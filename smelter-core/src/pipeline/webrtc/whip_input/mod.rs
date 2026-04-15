use std::sync::Arc;
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    PipelineCtx,
    pipeline::{
        rtp::RtpJitterBufferSharedContext,
        webrtc::{WhipWhepServerState, peer_connection_recvonly::OnTrackHdlrContext},
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
    jitter_buffer_ctx: RtpJitterBufferSharedContext,
}

impl WhipTrackContext {
    fn new(
        track_ctx: OnTrackHdlrContext,
        state: &WhipWhepServerState,
        buffer: &RtpJitterBufferSharedContext,
    ) -> Self {
        Self {
            track: track_ctx.track,
            rtc_receiver: track_ctx.rtc_receiver,
            pipeline_ctx: state.ctx.clone(),
            jitter_buffer_ctx: buffer.clone(),
        }
    }
}
