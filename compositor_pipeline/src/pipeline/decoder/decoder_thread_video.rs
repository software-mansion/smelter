use std::sync::Arc;

use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level, Span};

use crate::{
    error::DecoderInitError,
    pipeline::decoder::{DecoderThreadHandle, VideoDecoderStream},
    prelude::EncodedInputChunk,
    PipelineCtx, PipelineEvent,
};

use super::VideoDecoder;

pub trait ThreadProcess {
    type InitOptions: Send + 'static;
    type InitOutput: Send + 'static;

    fn init(opts: Self::InitOptions) -> Self::InitOutput;
    fn run();
    fn thread_info(&self) -> (String, Span);
}

pub trait DecoderProcess: VideoDecoder + ThreadProcess {
    fn init(opts: Self::InitOptions) -> Self::InitOutput {
        VideoDecoderStream::<Decoder, _>::new(ctx, chunk_stream)
    }
}

impl<Decoder: DecoderProcess + Send + 'static> ThreadProcess for Decoder {
    type InitOptions = (
        Arc<PipelineCtx>,
        crossbeam_channel::IntoIter<PipelineEvent<EncodedInputChunk>>,
    );
    type InitOutput = Result<
        VideoDecoderStream<Decoder, crossbeam_channel::IntoIter<PipelineEvent<EncodedInputChunk>>>,
        DecoderInitError,
    >;

    fn init(opts: Self::InitOptions) -> Self::InitOutput {
        let (ctx, chunk_stream) = opts;

        Ok(())
    }
}

pub fn spawn_thread<Process: ThreadProcess>(
    opts: Process::InitOptions,
    process: Process,
) -> Process::InitOutput {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    let (thread_name, thread_span) = process.thread_info();
    std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let _span = thread_span.entered();
            let result = Process::init(opts);
            result_sender.send(result).unwrap();
            Process::run();
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

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

//     let decoded_stream =V
//         VideoDecoderStream::<Decoder, _>::new(ctx, transformed_bytestream)?.flatten();

//     Ok((decoded_stream, DecoderThreadHandle { chunk_sender }))
// }
