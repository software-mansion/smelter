use std::sync::Arc;

use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    error::DecoderInitError,
    pipeline::{
        decoder::{VideoDecoder, VideoDecoderStream},
        input::rtp::depayloader::{DepayloaderOptions, DepayloaderStream},
        output::rtp::RtpPacket,
        PipelineCtx,
    },
    queue::PipelineEvent,
};

pub(crate) struct RtpVideoTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpPacket>>,
}

pub fn spawn_rtp_video_thread<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    depayloader_options: DepayloaderOptions,
    frame_sender: Sender<PipelineEvent<Frame>>,
) -> Result<RtpVideoTrackThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("RTP video track thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
            )
            .entered();

            let result = init_stream::<Decoder>(ctx, depayloader_options);
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
                if frame_sender.send(event).is_err() {
                    warn!("Failed to send encoded video chunk from decoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Decoder: VideoDecoder>(
    ctx: Arc<PipelineCtx>,
    depayloader_options: DepayloaderOptions,
) -> Result<
    (
        impl Iterator<Item = PipelineEvent<Frame>>,
        RtpVideoTrackThreadHandle,
    ),
    DecoderInitError,
> {
    let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);

    let depayloader_stream =
        DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());

    let decoder_stream = VideoDecoderStream::<Decoder, _>::new(ctx, depayloader_stream.flatten())?;

    Ok((
        decoder_stream.flatten(),
        RtpVideoTrackThreadHandle { rtp_packet_sender },
    ))
}
