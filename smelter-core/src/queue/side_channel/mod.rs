mod serialize;
mod server;

use std::path::PathBuf;

use crossbeam_channel::TrySendError;
use smelter_render::Frame;
use tracing::{debug, trace};

use crate::{prelude::InputAudioSamples, queue::queue_input::TrackOffset};

use super::SharedPts;

use self::{
    serialize::{serialize_audio_batch, serialize_frame},
    server::SideChannelServer,
};

#[derive(Clone)]
pub struct VideoSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    server: SideChannelServer,
}

impl VideoSideChannel {
    pub fn new(socket_path: PathBuf, start_pts: SharedPts) -> Self {
        Self {
            track_offset: TrackOffset::default(),
            start_pts,
            server: SideChannelServer::new(socket_path, "video-sc", 1),
        }
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
        let Some(data) = serialize_frame(&frame) else {
            trace!("Skipping side channel for GPU-only frame format");
            return;
        };
        if let Err(TrySendError::Full(_)) = self.server.sender.try_send(data) {
            debug!("Video side channel: dropping frame, channel full");
        }
    }
}

#[derive(Clone)]
pub struct AudioSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    server: SideChannelServer,
}

impl AudioSideChannel {
    pub fn new(socket_path: PathBuf, start_pts: SharedPts) -> Self {
        Self {
            track_offset: TrackOffset::default(),
            start_pts,
            server: SideChannelServer::new(socket_path, "audio-sc", 10),
        }
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
        let data = serialize_audio_batch(&batch);
        if let Err(TrySendError::Full(_)) = self.server.sender.try_send(data) {
            debug!("Audio side channel: dropping samples, channel full");
        }
    }
}
