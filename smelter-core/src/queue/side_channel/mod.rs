mod serialize;
mod server;

use std::{path::Path, sync::Arc};

use crossbeam_channel::TrySendError;
use smelter_render::{Frame, InputId};
use tracing::{debug, info};

use crate::{
    pipeline::PipelineCtx, prelude::InputAudioSamples, queue::queue_input::TrackOffset, types::Ref,
};

use super::SharedPts;

use self::server::{AudioSideChannelServer, VideoSideChannelServer};

#[derive(Clone)]
pub struct VideoSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    server: VideoSideChannelServer,
}

impl VideoSideChannel {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        socket_dir: &Path,
    ) -> Option<Self> {
        let path = socket_dir.join(format!("video_{}.sock", input_ref.id()));
        info!(?path, "Starting video side channel");
        let server = VideoSideChannelServer::new(path, input_ref.id(), ctx.wgpu_ctx.clone())?;
        Some(Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            server,
        })
    }

    pub(super) fn with_track_offset(&self, track_offset: &TrackOffset) -> Self {
        let mut side_channel = self.clone();
        side_channel.track_offset = track_offset.clone();
        side_channel
    }

    pub(super) fn send_frame(&mut self, frame: &Frame) {
        let Some(offset) = self.track_offset.get() else {
            return;
        };
        let Some(start_pts) = self.start_pts.value() else {
            return;
        };
        let mut frame = frame.clone();
        frame.pts = frame.pts + offset - start_pts;
        if let Err(TrySendError::Full(_)) = self.server.sender.try_send(frame) {
            debug!("Video side channel: dropping frame, channel full");
        }
    }
}

#[derive(Clone)]
pub struct AudioSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    server: AudioSideChannelServer,
}

impl AudioSideChannel {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        socket_dir: &Path,
    ) -> Option<Self> {
        let path = socket_dir.join(format!("audio_{}.sock", input_ref.id()));
        info!(?path, "Starting audio side channel");
        let server = AudioSideChannelServer::new(path, input_ref.id())?;
        Some(Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            server,
        })
    }

    pub(super) fn with_track_offset(&self, track_offset: &TrackOffset) -> Self {
        let mut clone = self.clone();
        clone.track_offset = track_offset.clone();
        clone
    }

    pub(super) fn send_samples(&self, batch: &InputAudioSamples) {
        let Some(offset) = self.track_offset.get() else {
            return;
        };
        let Some(start_pts) = self.start_pts.value() else {
            return;
        };
        let mut batch = batch.clone();
        batch.start_pts = batch.start_pts + offset - start_pts;
        if let Err(TrySendError::Full(_)) = self.server.sender.try_send(batch) {
            debug!("Audio side channel: dropping samples, channel full");
        }
    }
}
