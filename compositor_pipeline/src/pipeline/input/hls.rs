use std::{
    ffi::CString,
    ptr, slice,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::{
    error::InputInitError,
    pipeline::{
        decoder::{
            self, decoder_thread_audio::spawn_audio_decoder_thread,
            decoder_thread_video::spawn_video_decoder_thread, fdk_aac, ffmpeg_h264,
            DecodedDataReceiver, DecoderThreadHandle,
        },
        input::hls::chunk_repacker::ChunkRepacker,
        types::{EncodedChunk, IsKeyframe},
        AudioCodec, EncodedChunkKind, PipelineCtx, VideoCodec,
    },
    queue::PipelineEvent,
};
use bytes::Bytes;
use compositor_render::InputId;
use crossbeam_channel::{bounded, Sender};
use ffmpeg_next::{
    ffi::{
        avformat_alloc_context, avformat_close_input, avformat_find_stream_info,
        avformat_open_input,
    },
    format::context,
    media::Type,
    util::interrupt,
    Dictionary, Packet,
};
use tracing::{debug, error, span, warn, Level};

use super::{Input, InputInitInfo};

mod chunk_repacker;

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoder: decoder::VideoDecoderOptions,
}

pub struct HlsInput {
    should_close: Arc<AtomicBool>,
}

impl HlsInput {
    const PREFERABLE_BUFFER_SIZE: usize = 30;
    const MIN_BUFFER_SIZE: usize = Self::PREFERABLE_BUFFER_SIZE / 2;

    pub(super) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: HlsInputOptions,
    ) -> Result<(Input, InputInitInfo, DecodedDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let receivers = Self::spawn_thread(ctx, input_id, should_close.clone(), opts)?;

        Ok((
            Input::Hls(Self { should_close }),
            InputInitInfo::Other,
            receivers,
        ))
    }

    fn spawn_thread(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        should_close: Arc<AtomicBool>,
        options: HlsInputOptions,
    ) -> Result<DecodedDataReceiver, InputInitError> {
        let (result_sender, result_receiver) = bounded(1);
        std::thread::Builder::new()
            .name(format!("HLS thread for input {}", input_id.clone()))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "HLS thread", input_id = input_id.to_string()).entered();

                Self::run_thread(ctx, input_id, options, should_close, result_sender);
            })
            .unwrap();

        result_receiver.recv().unwrap()
    }

    #[allow(clippy::type_complexity)]
    fn run_thread(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        options: HlsInputOptions,
        should_close: Arc<AtomicBool>,
        result_sender: Sender<Result<DecodedDataReceiver, InputInitError>>,
    ) {
        // careful: moving the input context in any way will cause ffmpeg to segfault
        // I do not know why this happens
        let mut input_ctx = match input_with_dictionary_and_interrupt(
            &options.url,
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
            || should_close.load(Ordering::Relaxed),
        ) {
            Ok(i) => i,
            Err(e) => {
                result_sender
                    .send(Err(InputInitError::FfmpegError(e)))
                    .unwrap();
                return;
            }
        };

        let input_start_time = Instant::now();

        let (mut audio, mut samples_receiver) = match input_ctx.streams().best(Type::Audio) {
            Some(stream) => {
                // not tested it was always null, but audio is in ADTS, so config is not
                // necessary
                let asc = unsafe {
                    let codecpar = (*stream.as_ptr()).codecpar;
                    let size = (*codecpar).extradata_size;
                    if size > 0 {
                        Some(bytes::Bytes::copy_from_slice(slice::from_raw_parts(
                            (*codecpar).extradata,
                            size as usize,
                        )))
                    } else {
                        None
                    }
                };
                let (samples_sender, samples_receiver) = bounded(5);
                let state =
                    StreamState::new(input_start_time, ctx.queue_sync_time, stream.time_base());
                let decoder_result = spawn_audio_decoder_thread::<fdk_aac::FdkAacDecoder, 2000>(
                    ctx.clone(),
                    input_id.clone(),
                    fdk_aac::Options { asc },
                    samples_sender,
                );
                let handle = match decoder_result {
                    Ok(handle) => handle,
                    Err(err) => {
                        result_sender.send(Err(err.into())).unwrap();
                        return;
                    }
                };
                (
                    Some((stream.index(), handle, state)),
                    Some(samples_receiver),
                )
            }
            None => (None, None),
        };
        let (mut video, mut frame_receiver) = match input_ctx.streams().best(Type::Video) {
            Some(stream) => {
                let (frame_sender, frame_receiver) = bounded(5);
                let state =
                    StreamState::new(input_start_time, ctx.queue_sync_time, stream.time_base());
                let decoder_result = spawn_video_decoder_thread::<
                    ffmpeg_h264::FfmpegH264Decoder,
                    2000,
                >(ctx.clone(), input_id.clone(), frame_sender);
                let handle = match decoder_result {
                    Ok(handle) => handle,
                    Err(err) => {
                        result_sender.send(Err(err.into())).unwrap();
                        return;
                    }
                };
                let repacker = ChunkRepacker::new(&stream);
                (
                    Some((stream.index(), handle, state, repacker)),
                    Some(frame_receiver),
                )
            }
            None => (None, None),
        };

        let mut is_buffering = true;
        let mut pts_offset = Duration::ZERO;
        loop {
            let mut packet = Packet::empty();
            match packet.read(&mut input_ctx) {
                Ok(_) => (),
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(err) => {
                    warn!("HLS read error {err:?}");
                    continue;
                }
            }

            if packet.is_corrupt() {
                error!(
                    "Corrupted packet {:?} {:?}",
                    packet.stream(),
                    packet.flags()
                );
                continue;
            }

            if let Some((index, ref handle, ref mut state, ref mut repacker)) = video {
                if packet.stream() == index {
                    let (pts, dts, is_discontinuity) = state.pts_dts_from_packet(&packet);

                    let data = Bytes::copy_from_slice(packet.data().unwrap());
                    let data = repacker.repack(data);

                    // resulting in no video or blinking. This heuritic moves the next packets forward in
                    // time. We only care about video buffer, audio uses the same offset as video
                    // to avoid audio sync issues.
                    if is_discontinuity {
                        pts_offset = Duration::ZERO;
                    } else if !is_buffering && handle.chunk_sender.len() < HlsInput::MIN_BUFFER_SIZE
                    {
                        pts_offset += Duration::from_secs_f64(0.1);
                    }
                    let pts = pts + pts_offset;

                    let chunk = EncodedChunk {
                        data,
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Video(VideoCodec::H264),
                    };

                    if handle.chunk_sender.is_empty() {
                        debug!("HLS input video channel was drained");
                    }
                    if handle
                        .chunk_sender
                        .send(PipelineEvent::Data(chunk))
                        .is_err()
                    {
                        debug!("Channel closed")
                    }
                }
            }

            if let Some((index, ref sender, ref mut state)) = audio {
                if packet.stream() == index {
                    let (pts, dts, _) = state.pts_dts_from_packet(&packet);
                    let pts = pts + pts_offset;

                    let chunk = EncodedChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Audio(AudioCodec::Aac),
                    };

                    if sender.chunk_sender.is_empty() {
                        debug!("HLS input audio channel was drained");
                    }
                    if sender
                        .chunk_sender
                        .send(PipelineEvent::Data(chunk))
                        .is_err()
                    {
                        debug!("Channel closed")
                    }
                }
            }

            if is_buffering && HlsInput::did_buffer_enough(video.as_ref(), audio.as_ref()) {
                result_sender
                    .send(Ok(DecodedDataReceiver {
                        video: frame_receiver.take(),
                        audio: samples_receiver.take(),
                    }))
                    .unwrap();

                is_buffering = false;
            }
        }

        if let Some((_, handle, _)) = audio {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }

        if let Some((_, handle, _, _)) = video {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }
    }

    fn did_buffer_enough(
        video: Option<&(usize, DecoderThreadHandle, StreamState, ChunkRepacker)>,
        audio: Option<&(usize, DecoderThreadHandle, StreamState)>,
    ) -> bool {
        let is_video_ready = video
            .as_ref()
            .map(|(_, handle, _, _)| handle.chunk_sender.len() >= HlsInput::PREFERABLE_BUFFER_SIZE);
        let is_audio_ready = audio
            .as_ref()
            .map(|(_, handle, _)| handle.chunk_sender.len() >= HlsInput::PREFERABLE_BUFFER_SIZE);

        matches!(
            (is_video_ready, is_audio_ready),
            (Some(true), Some(true)) | (Some(true), None) | (None, Some(true))
        )
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct StreamState {
    input_start_time: Instant,
    queue_start_time: Instant,
    first_pts: Option<Duration>,

    prev_dts: Option<f64>,
    next_predicted_dts: Option<f64>,
    discontinuity_offset: f64,
    time_base: ffmpeg_next::Rational,
}

impl StreamState {
    /// (10s) This value was picked arbitrarily but it's quite conservative.
    const DISCONTINUITY_THRESHOLD: f64 = 10.0;

    fn new(
        input_start_time: Instant,
        queue_start_time: Instant,
        time_base: ffmpeg_next::Rational,
    ) -> Self {
        Self {
            input_start_time,
            queue_start_time,
            first_pts: None,
            prev_dts: None,
            next_predicted_dts: None,
            discontinuity_offset: 0.0,
            time_base,
        }
    }

    fn detect_discontinuity(&mut self, packet: &Packet) -> bool {
        let dts = packet.dts().unwrap_or(0) as f64;
        let (Some(prev_dts), Some(next_dts)) = (self.prev_dts, self.next_predicted_dts) else {
            self.prev_dts = Some(dts);
            self.next_predicted_dts = Some(dts + packet.duration() as f64);
            return false;
        };

        // Detect discontinuity
        let timestamp_delta = self.to_timestamp(f64::abs(next_dts - dts)).as_secs_f64();
        let is_discontinuity = timestamp_delta >= Self::DISCONTINUITY_THRESHOLD || prev_dts > dts;
        if is_discontinuity {
            debug!("Discontinuity detected: {prev_dts} -> {dts} (dts)");
            self.discontinuity_offset += next_dts - dts;
        }

        self.prev_dts = Some(dts);
        self.next_predicted_dts = Some(dts + packet.duration() as f64);

        is_discontinuity
    }

    fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>, bool) {
        let is_discontinuity = self.detect_discontinuity(packet);
        let pts = self.to_timestamp(packet.pts().unwrap_or(0) as f64 + self.discontinuity_offset);
        let dts = packet
            .dts()
            .map(|dts| self.to_timestamp(dts as f64 + self.discontinuity_offset));

        // Recalculate pts in regards to queue start time
        let first_pts = *self.first_pts.get_or_insert(pts);
        let pts = self.to_queue_timestamp(pts.saturating_sub(first_pts));

        (pts, dts, is_discontinuity)
    }

    fn to_timestamp(&self, timestamp: f64) -> Duration {
        Duration::from_secs_f64(
            f64::max(timestamp, 0.0) * self.time_base.numerator() as f64
                / self.time_base.denominator() as f64,
        )
    }

    fn to_queue_timestamp(&self, input_timestamp: Duration) -> Duration {
        (self.input_start_time + input_timestamp).duration_since(self.queue_start_time)
    }
}

/// Combined implementation of ffmpeg_next::format:input_with_interrupt and
/// ffmpeg_next::format::input_with_dictionary that allows passing both interrupt
/// callback and Dictionary with options
fn input_with_dictionary_and_interrupt<F>(
    path: &str,
    options: Dictionary,
    interrupt_fn: F,
) -> Result<context::Input, ffmpeg_next::Error>
where
    F: FnMut() -> bool,
{
    unsafe {
        let mut ps = avformat_alloc_context();

        (*ps).interrupt_callback = interrupt::new(Box::new(interrupt_fn)).interrupt;

        let path = CString::new(path).unwrap();
        let mut opts = options.disown();
        let res = avformat_open_input(&mut ps, path.as_ptr(), ptr::null_mut(), &mut opts);

        Dictionary::own(opts);

        match res {
            0 => match avformat_find_stream_info(ps, ptr::null_mut()) {
                r if r >= 0 => Ok(context::Input::wrap(ps)),
                e => {
                    avformat_close_input(&mut ps);
                    Err(ffmpeg_next::Error::from(e))
                }
            },

            e => Err(ffmpeg_next::Error::from(e)),
        }
    }
}
