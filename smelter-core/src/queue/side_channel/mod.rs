mod serialize;
mod server;

use std::{path::Path, sync::Arc};

use crossbeam_channel::{Sender, TrySendError};
use smelter_render::{Frame, InputId};
use tracing::{debug, info};

use crate::{
    pipeline::PipelineCtx, prelude::InputAudioSamples, queue::queue_input::TrackOffset, types::Ref,
};

use super::SharedPts;

use self::server::SideChannelServer;

/// Where the side channel data is sent to. `UnixSocket` owns the server, so
/// dropping the side channel shuts it down and removes the socket file.
#[derive(Clone)]
enum SideChannelSink<T> {
    Native(Sender<T>),
    UnixSocket(SideChannelServer<T>),
}

impl<T> SideChannelSink<T> {
    fn sender(&self) -> &Sender<T> {
        match self {
            Self::Native(sender) => sender,
            Self::UnixSocket(server) => server.sender(),
        }
    }
}

#[derive(Clone)]
pub struct VideoSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    sink: SideChannelSink<Frame>,
}

impl VideoSideChannel {
    pub(super) fn unix_socket(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        socket_dir: &Path,
    ) -> Option<Self> {
        let path = socket_dir.join(format!("video_{}.sock", input_ref.id()));
        info!(?path, "Starting video side channel");
        let server = SideChannelServer::new_video(path, input_ref.id(), ctx.wgpu_ctx.clone())?;
        Some(Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            sink: SideChannelSink::UnixSocket(server),
        })
    }

    pub(super) fn native(ctx: &Arc<PipelineCtx>, sender: Sender<Frame>) -> Self {
        Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            sink: SideChannelSink::Native(sender),
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
        frame.pts = (frame.pts + offset).saturating_sub(start_pts);
        if let Err(TrySendError::Full(_)) = self.sink.sender().try_send(frame) {
            debug!("Video side channel: dropping frame, channel full");
        }
    }
}

#[derive(Clone)]
pub struct AudioSideChannel {
    track_offset: TrackOffset,
    start_pts: SharedPts,
    sink: SideChannelSink<InputAudioSamples>,
}

impl AudioSideChannel {
    pub(super) fn unix_socket(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        socket_dir: &Path,
    ) -> Option<Self> {
        let path = socket_dir.join(format!("audio_{}.sock", input_ref.id()));
        info!(?path, "Starting audio side channel");
        let server = SideChannelServer::new_audio(path, input_ref.id())?;
        Some(Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            sink: SideChannelSink::UnixSocket(server),
        })
    }

    pub(super) fn native(ctx: &Arc<PipelineCtx>, sender: Sender<InputAudioSamples>) -> Self {
        Self {
            track_offset: TrackOffset::default(),
            start_pts: ctx.queue_ctx.start_pts.clone(),
            sink: SideChannelSink::Native(sender),
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
        batch.start_pts = (batch.start_pts + offset).saturating_sub(start_pts);
        if let Err(TrySendError::Full(_)) = self.sink.sender().try_send(batch) {
            debug!("Audio side channel: dropping samples, channel full");
        }
    }
}
