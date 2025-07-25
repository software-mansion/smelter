use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    error::DecoderInitError,
    pipeline::decoder::{DecoderThreadHandle, VideoDecoderStream},
    prelude::EncodedInputChunk,
    PipelineEvent,
};

use super::VideoDecoder;

pub fn spawn_video_decoder_thread<Decoder, const BUFFER_SIZE: usize, Source, InitStreamFn>(
    input_id: InputId,
    frame_sender: Sender<PipelineEvent<Frame>>,
    init_stream: InitStreamFn,
) -> Result<DecoderThreadHandle, DecoderInitError>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
    InitStreamFn: FnOnce(
            crossbeam_channel::IntoIter<PipelineEvent<EncodedInputChunk>>,
        ) -> Result<VideoDecoderStream<Decoder, Source>, DecoderInitError>
        + Send
        + 'static,
{
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Video decoder thread",
                input_id = input_id.to_string(),
                decoder = Decoder::LABEL
            )
            .entered();

            let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(BUFFER_SIZE);

            let result = init_stream(chunk_receiver.into_iter());
            let stream = match result {
                Ok(stream) => {
                    result_sender
                        .send(Ok(DecoderThreadHandle { chunk_sender }))
                        .unwrap();
                    stream
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in stream.flatten() {
                if frame_sender.send(event).is_err() {
                    warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Decoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

// fn init_decoder_stream<
//     Decoder: VideoDecoder,
//     const BUFFER_SIZE: usize,
//     Transformer: BytestreamTransformer,
// >(
//     ctx: Arc<PipelineCtx>,
//     transformer: Option<Transformer>,
// ) -> Result<
//     (
//         impl Iterator<Item = PipelineEvent<Frame>>,
//         DecoderThreadHandle,
//     ),
//     DecoderInitError,
// > {
//     let transformed_bytestream =
//         BytestreamTransformStream::new(transformer, chunk_receiver.into_iter());

//     let decoded_stream =
//         VideoDecoderStream::<Decoder, _>::new(ctx, transformed_bytestream)?.flatten();

//     Ok((decoded_stream, DecoderThreadHandle { chunk_sender }))
// }
