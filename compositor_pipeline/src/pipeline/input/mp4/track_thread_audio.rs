use std::{fs::File, sync::Arc, thread, time::Duration};

use compositor_render::InputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::InputSamples,
    error::DecoderInitError,
    pipeline::{
        decoder::{
            fdk_aac::{self, FdkAacDecoder},
            AudioDecoder, AudioDecoderStream,
        },
        input::mp4::{reader::Track, TrackState},
        resampler::decoder_resampler::ResampledDecoderStream,
        types::DecodedSamples,
        EncodedChunk, PipelineCtx,
    },
    queue::PipelineEvent,
};

pub(crate) struct AudioTrackThreadHandle {
    thread_handle: thread::JoinHandle<()>,
}

impl AudioTrackThreadHandle {
    pub fn join(&self) -> TrackState {
        self.thread_handle.join().unwrap()
    }
}

pub fn spawn_audio_track_thread(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    offset: Duration,
    track: Arc<Track<File>>,
    chunks_sender: Sender<PipelineEvent<InputSamples>>,
    finished_track_sender: Sender<()>,
) -> Result<AudioTrackThreadHandle, DecoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    let handle = std::thread::Builder::new()
        .name(format!("Decoder thread for input {}", &input_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "MP4 audio track thread",
                input_id = input_id.to_string(),
            )
            .entered();

            let result = AudioTrackThread::new(ctx, offset, track);
            let thread_state = match result {
                Ok(thread_state) => {
                    result_sender.send(Ok(())).unwrap();
                    thread_state
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in thread_state.iterator {
                if chunks_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
                    break;
                }
            }
            let _ = finished_track_sender.send(());
            debug!("Decoder thread finished.");
            thread_state.final_state()
        })
        .unwrap();

    result_receiver.recv().unwrap()?;
    Ok(AudioTrackThreadHandle {
        thread_handle: handle,
    })
}

struct AudioTrackThread {
    ctx: Arc<PipelineCtx>,
    state: TrackState,
    track: Arc<Track<File>>,
    asc: bytes::Bytes,
}

impl AudioTrackThread {
    fn new(
        ctx: Arc<PipelineCtx>,
        initial_offset: Duration,
        track: Arc<Track<File>>,
        asc: bytes::Bytes,
    ) -> Self {
        Self {
            state: TrackState::new(initial_offset),
            track,
            ctx,
            asc,
        }
    }

    fn result_iter<'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = PipelineEvent<InputSamples>> + 'a, DecoderInitError> {
        let chunks_stream = self
            .track
            .chunks()
            .into_iter()
            .map(|(chunk, _duration)| PipelineEvent::Data(chunk));

        let decoded_stream = AudioDecoderStream::<FdkAacDecoder, _>::new(
            self.ctx.clone(),
            fdk_aac::Options {
                asc: Some(self.asc.clone()),
            },
            chunks_stream,
        )?;

        let resampled_stream =
            ResampledDecoderStream::new(self.ctx.mixing_sample_rate, decoded_stream.flatten());

        Ok(resampled_stream.flatten())
    }

    fn final_state(self) -> TrackState {
        self.state
    }
}
