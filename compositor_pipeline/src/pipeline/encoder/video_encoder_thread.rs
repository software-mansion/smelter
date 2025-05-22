use std::sync::Arc;

use compositor_render::{Frame, OutputId, Resolution};
use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, span, warn, Level};

use crate::{
    error::EncoderInitError,
    pipeline::{EncoderOutputEvent, PipelineCtx},
    queue::PipelineEvent,
};

use super::{VideoEncoder, VideoEncoderConfig};

pub(crate) struct VideoEncoderThread<Encoder: VideoEncoder> {
    encoder: Encoder,
    frame_receiver: Receiver<PipelineEvent<Frame>>,
    keyframe_request_receiver: Receiver<()>,
    chunks_sender: Sender<EncoderOutputEvent>,
}

pub(crate) struct VideoEncoderThreadHandle {
    frame_sender: Sender<PipelineEvent<Frame>>,
    keyframe_request_sender: Sender<()>,
    config: VideoEncoderConfig,
}

impl<Encoder: VideoEncoder> VideoEncoderThread<Encoder> {
    pub fn spawn(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: Encoder::Options,
        chunks_sender: Sender<EncoderOutputEvent>,
    ) -> Result<VideoEncoderThreadHandle, EncoderInitError> {
        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
        let (result_sender, result_receiver) = crossbeam_channel::bounded(0);
        let (keyframe_request_sender, keyframe_request_receiver) = crossbeam_channel::unbounded();

        std::thread::Builder::new()
            .name(format!("Encoder thread for output {}", &output_id))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "Encoder thread",
                    output_id = output_id.to_string(),
                    encoder = Encoder::LABEL
                )
                .entered();

                let result = Self::new(
                    ctx,
                    options,
                    chunks_sender,
                    frame_receiver,
                    keyframe_request_receiver,
                );
                match result {
                    Ok((encoder, config)) => {
                        result_sender.send(Ok(config));
                        encoder.run()
                    }
                    Err(err) => {
                        result_sender.send(Err(err)).unwrap();
                    }
                };
            })
            .unwrap();

        let config = result_receiver.recv().unwrap()?;

        Ok(VideoEncoderThreadHandle {
            frame_sender,
            keyframe_request_sender,
            config,
        })
    }

    fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        chunks_sender: Sender<EncoderOutputEvent>,
        frame_receiver: Receiver<PipelineEvent<Frame>>,
        keyframe_request_receiver: Receiver<()>,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        let (encoder, config) = Encoder::new(&ctx, options)?;
        Ok((
            Self {
                encoder,
                chunks_sender,
                frame_receiver,
                keyframe_request_receiver,
            },
            config,
        ))
    }

    fn run(mut self) {
        loop {
            let frame = match self.frame_receiver.recv() {
                Ok(PipelineEvent::Data(f)) => f,
                Ok(PipelineEvent::EOS) => break,
                Err(_) => break,
            };
            if self.has_keyframe_request() {
                self.encoder.request_keyframe()
            }
            let chunks = self.encoder.encode(frame);
            for chunk in chunks {
                if self
                    .chunks_sender
                    .send(EncoderOutputEvent::Data(chunk))
                    .is_err()
                {
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
        }

        let flushed = self.encoder.flush();
        for chunk in flushed {
            if self
                .chunks_sender
                .send(EncoderOutputEvent::Data(chunk))
                .is_err()
            {
                warn!("Failed to send encoded video. Channel closed.");
                return;
            }
        }
        if let Err(_err) = self.chunks_sender.send(EncoderOutputEvent::VideoEOS) {
            warn!("Failed to send EOS. Channel closed.")
        }
        debug!("Encoder thread finished.");
    }

    fn has_keyframe_request(&self) -> bool {
        let mut has_keyframe_request = false;
        while self.keyframe_request_receiver.try_recv().is_ok() {
            has_keyframe_request = true;
        }
        return has_keyframe_request;
    }
}

impl VideoEncoderThreadHandle {
    pub fn frame_sender(&self) -> &Sender<PipelineEvent<Frame>> {
        &self.frame_sender
    }

    pub fn resolution(&self) -> Resolution {
        self.config.resolution
    }

    pub fn keyframe_request_sender(&self) -> &Sender<()> {
        &self.keyframe_request_sender
    }

    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
