use std::sync::Arc;

use compositor_render::{Frame, OutputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::pipeline::{
    encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
    rtp::payloader::{PayloaderOptions, PayloaderStream},
};
use crate::prelude::*;

use super::RtpEvent;

pub(crate) struct RtpVideoTrackThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub fn spawn_rtp_video_thread<Encoder: VideoEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
    chunks_sender: Sender<RtpEvent>,
) -> Result<RtpVideoTrackThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("RTP video track thread for output {}", &output_id))
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
                if chunks_sender.send(event).is_err() {
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
) -> Result<(impl Iterator<Item = RtpEvent>, RtpVideoTrackThreadHandle), EncoderInitError> {
    let ssrc = payloader_options.ssrc;
    let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);

    let (encoded_stream, encoder_ctx) =
        VideoEncoderStream::<Encoder, _>::new(ctx, encoder_options, frame_receiver.into_iter())?;

    let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

    let stream = payloaded_stream.flatten().map(move |event| match event {
        Ok(PipelineEvent::Data(packet)) => RtpEvent::Data(packet),
        Ok(PipelineEvent::EOS) => RtpEvent::VideoEos(rtcp::goodbye::Goodbye {
            sources: vec![ssrc],
            reason: bytes::Bytes::from("Unregister output stream"),
        }),
        Err(err) => RtpEvent::Err(err),
    });

    Ok((
        stream,
        RtpVideoTrackThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        },
    ))
}
