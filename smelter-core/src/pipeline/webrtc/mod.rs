use tokio::sync::oneshot;
use webrtc::{
    api::setting_engine::SettingEngine,
    ice::udp_network::{EphemeralUDP, UDPNetwork},
    ice_transport::ice_candidate_type::RTCIceCandidateType,
};

use std::sync::Arc;
use tracing::{error, info};

mod bearer_token;
mod error;
mod handle_keyframe_requests;
mod http_client;
mod input_rtcp_listener;
mod input_rtp_reader;
mod input_thread;
mod negotiated_codecs;
mod peer_connection_recvonly;
mod server;
mod supported_codec_parameters;
mod trickle_ice_utils;

mod whep_input;
mod whep_output;
mod whip_input;
mod whip_output;

pub(super) use server::WhipWhepServer;
pub(super) use whep_input::WhepInput;
pub(super) use whep_output::WhepOutput;
pub(super) use whip_input::WhipInput;
pub(super) use whip_output::WhipOutput;

use crate::pipeline::{
    PipelineCtx,
    webrtc::{whep_output::state::WhepOutputsState, whip_input::state::WhipInputsState},
};

#[derive(Debug, Clone)]
struct WhipWhepServerState {
    inputs: WhipInputsState,
    outputs: WhepOutputsState,
    ctx: Arc<PipelineCtx>,
}

#[derive(Debug)]
pub struct WhipWhepPipelineState {
    pub port: u16,
    pub inputs: WhipInputsState,
    pub outputs: WhepOutputsState,
}

impl WhipWhepPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: WhipInputsState::default(),
            outputs: WhepOutputsState::default(),
        })
    }
}

#[derive(Debug)]
pub struct WhipWhepServerHandle {
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl Drop for WhipWhepServerHandle {
    fn drop(&mut self) {
        info!("Stopping WHIP/WHEP server");
        if let Some(sender) = self.shutdown_sender.take()
            && sender.send(()).is_err()
        {
            error!("Cannot send shutdown signal to WHIP/WHEP server")
        }
    }
}

pub struct AsyncReceiverIter<T> {
    pub receiver: tokio::sync::mpsc::Receiver<T>,
}

impl<T> Iterator for AsyncReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.blocking_recv()
    }
}

fn default_setting_engine(ctx: &Arc<PipelineCtx>) -> SettingEngine {
    let mut setting_engine = SettingEngine::default();
    if !ctx.webrtc_nat_1to1_ips.is_empty() {
        setting_engine
            .set_nat_1to1_ips(ctx.webrtc_nat_1to1_ips.to_vec(), RTCIceCandidateType::Host);
    }

    if let Some((start, end)) = ctx.webrtc_port_range {
        let mut ephemeral_udp = EphemeralUDP::default();
        ephemeral_udp
            .set_ports(start, u16::max(end, start))
            .unwrap(); // It can only fail if start>port
        setting_engine.set_udp_network(UDPNetwork::Ephemeral(ephemeral_udp));
    }
    setting_engine
}
