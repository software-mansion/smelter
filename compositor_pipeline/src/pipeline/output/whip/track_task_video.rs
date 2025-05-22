use std::sync::Arc;

use compositor_render::{Frame, OutputFrameFormat, OutputId, Resolution};
use crossbeam_channel::Sender;
use tokio::sync::mpsc;
use tracing::{debug, span, warn, Level};

use crate::{
    error::EncoderInitError,
    pipeline::{
        encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
        output::rtp::{
            payloader::{PayloaderOptions, PayloaderStream},
            RtpEvent,
        },
        PipelineCtx,
    },
    queue::PipelineEvent,
};

pub(crate) struct WhipVideoTrackThreadHandle {
    frame_sender: Sender<PipelineEvent<Frame>>,
    keyframe_request_sender: Sender<()>,
    config: VideoEncoderConfig,
}

pub fn spawn_video_track_thread<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
    chunks_sender: mpsc::Sender<RtpEvent>,
) -> Result<WhipVideoTrackThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("WHIP video track thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Encoder thread",
                output_id = output_id.to_string(),
                encoder = Encoder::LABEL
            )
            .entered();

            let result = init_stream::<Encoder>(ctx, encoder_options, payloader_options);
            let stream = match result {
                Ok((stream, handle)) => {
                    result_sender.send(Ok(handle)).unwrap();
                    stream
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in stream {
                if chunks_sender.blocking_send(event).is_err() {
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
) -> Result<(impl Iterator<Item = RtpEvent>, WhipVideoTrackThreadHandle), EncoderInitError> {
    let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
    let (encoded_stream, encoder_ctx) =
        VideoEncoderStream::<Encoder, _>::new(ctx, encoder_options, frame_receiver.into_iter())?;

    let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

    let stream = payloaded_stream.flatten().map(|event| match event {
        Ok(PipelineEvent::Data(packet)) => RtpEvent::Data(packet),
        Ok(PipelineEvent::EOS) => RtpEvent::VideoEos,
        Err(err) => RtpEvent::Err(err),
    });

    Ok((
        stream,
        WhipVideoTrackThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        },
    ))
}

impl WhipVideoTrackThreadHandle {
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

    pub fn output_frame_format(&self) -> OutputFrameFormat {
        self.config.output_format
    }
}
