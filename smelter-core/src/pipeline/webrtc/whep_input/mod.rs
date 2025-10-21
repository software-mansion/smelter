use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    PipelineCtx,
    pipeline::{
        rtp::RtpNtpSyncPoint, utils::input_buffer::InputBuffer,
        webrtc::peer_connection_recvonly::OnTrackHdlrContext,
    },
};

mod input;
mod listen_for_trickle_candidates;
mod on_track;
mod resolve_video_preferences;

use std::sync::Arc;

pub(crate) use input::WhepInput;

#[derive(Clone)]
struct WhepTrackContext {
    track: Arc<TrackRemote>,
    rtc_receiver: Arc<RTCRtpReceiver>,
    pipeline_ctx: Arc<PipelineCtx>,
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
}

impl WhepTrackContext {
    fn new(
        track_ctx: OnTrackHdlrContext,
        pipeline_ctx: &Arc<PipelineCtx>,
        sync_point: &Arc<RtpNtpSyncPoint>,
        buffer: &InputBuffer,
    ) -> Self {
        Self {
            track: track_ctx.track,
            rtc_receiver: track_ctx.rtc_receiver,
            pipeline_ctx: pipeline_ctx.clone(),
            sync_point: sync_point.clone(),
            buffer: buffer.clone(),
        }
    }
}
