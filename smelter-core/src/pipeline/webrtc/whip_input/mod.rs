use std::sync::Arc;
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    PipelineCtx,
    pipeline::{
        rtp::{RtpJitterBufferInitOptions, RtpNtpSyncPoint},
        webrtc::{
            WhipWhepServerState, peer_connection_recvonly::OnTrackHdlrContext,
            whip_input::state::WhipInputsState,
        },
    },
};

mod input;
mod on_track;
mod video_preferences;

pub(super) mod create_new_session;
pub(super) mod state;

pub(crate) use input::WhipInput;

#[derive(Clone)]
struct WhipTrackContext {
    track: Arc<TrackRemote>,
    rtc_receiver: Arc<RTCRtpReceiver>,
    pipeline_ctx: Arc<PipelineCtx>,
    inputs: WhipInputsState,
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: RtpJitterBufferInitOptions,
}

impl WhipTrackContext {
    fn new(
        track_ctx: OnTrackHdlrContext,
        state: &WhipWhepServerState,
        sync_point: &Arc<RtpNtpSyncPoint>,
        buffer: &RtpJitterBufferInitOptions,
    ) -> Self {
        Self {
            track: track_ctx.track,
            rtc_receiver: track_ctx.rtc_receiver,
            pipeline_ctx: state.ctx.clone(),
            inputs: state.inputs.clone(),
            sync_point: sync_point.clone(),
            buffer: buffer.clone(),
        }
    }
}
