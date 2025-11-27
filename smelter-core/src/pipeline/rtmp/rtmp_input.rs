use std::sync::{Arc, atomic::AtomicBool};

use bytes::Bytes;
use crossbeam_channel::bounded;
use ffmpeg_next::Packet;
use smelter_render::InputId;
use tracing::{debug, trace};

use crate::{
    pipeline::{
        decoder::DecoderThreadHandle,
        input::Input,
        rtmp::rtmp_input::{input_loop::spawn_input_loop, stream_state::StreamState},
        utils::input_buffer::InputBuffer,
    },
    queue::QueueDataReceiver,
};

use crate::prelude::*;

mod demux;
mod ffmpeg_context;
mod ffmpeg_utils;
mod input_loop;
mod stream_state;

pub struct RtmpServerInput {
    should_close: Arc<AtomicBool>,
}

impl RtmpServerInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: RtmpServerInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let buffer = InputBuffer::new(&ctx, opts.buffer);

        let (frame_sender, frame_receiver) = bounded(10);
        let (samples_sender, samples_receiver) = bounded(10);

        let receivers = QueueDataReceiver {
            video: Some(frame_receiver),
            audio: Some(samples_receiver),
        };

        spawn_input_loop(
            ctx,
            input_ref,
            opts,
            should_close.clone(),
            buffer,
            frame_sender,
            samples_sender,
        );

        Ok((
            Input::RtmpServer(Self { should_close }),
            InputInitInfo::Other,
            receivers,
        ))
    }
}

impl Drop for RtmpServerInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct Track {
    index: usize,
    handle: DecoderThreadHandle,
    state: StreamState,
}

impl Track {
    fn send_packet(&mut self, packet: &Packet, kind: MediaKind) {
        let (pts, dts) = self.state.pts_dts_from_packet(packet);

        let chunk = EncodedInputChunk {
            data: Bytes::copy_from_slice(packet.data().unwrap()),
            pts,
            dts,
            kind,
        };

        let sender = &self.handle.chunk_sender;
        trace!(?chunk, buffer = sender.len(), "Sending chunk");
        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            debug!("Channel closed")
        }
    }
}
