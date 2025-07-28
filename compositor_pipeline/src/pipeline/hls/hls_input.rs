use std::{
    ffi::CString,
    ptr, slice,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
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
    Dictionary, Packet, Stream,
};
use tracing::{debug, error, info, span, trace, warn, Level};

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264,
            h264_utils::{AvccToAnnexBRepacker, H264AvcDecoderConfig},
            vulkan_h264,
        },
        input::Input,
    },
    queue::QueueDataReceiver,
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub struct HlsInput {
    should_close: Arc<AtomicBool>,
}

impl HlsInput {
    const PREFERABLE_BUFFER_SIZE: usize = 30;
    const MIN_BUFFER_SIZE: usize = Self::PREFERABLE_BUFFER_SIZE / 2;

    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: HlsInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
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
    ) -> Result<QueueDataReceiver, InputInitError> {
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
        result_sender: Sender<Result<QueueDataReceiver, InputInitError>>,
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

        let buffer_duration = options
            .buffer_duration
            .unwrap_or(Duration::from_secs_f64(10.0));

        let (mut audio, mut samples_receiver) = match input_ctx.streams().best(Type::Audio) {
            Some(stream) => {
                // not tested it was always null, but audio is in ADTS, so config is not
                // necessary
                let asc = read_extra_data(&stream);
                let (samples_sender, samples_receiver) = bounded(5);
                let state =
                    StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer_duration);
                let decoder_result = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
                    input_id.clone(),
                    AudioDecoderThreadOptions {
                        ctx: ctx.clone(),
                        decoder_options: FdkAacDecoderOptions { asc },
                        samples_sender,
                        input_buffer_size: 2000,
                    },
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
                    StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer_duration);

                let extra_data = read_extra_data(&stream);
                let h264_config = extra_data
                    .map(H264AvcDecoderConfig::parse)
                    .transpose()
                    .unwrap_or_else(|e| match e {
                        H264AvcDecoderConfigError::NotAVCC => None,
                        _ => {
                            warn!("Could not parse extra data: {e}");
                            None
                        }
                    });

                let decoder_thread_options = VideoDecoderThreadOptions {
                    ctx: ctx.clone(),
                    transformer: h264_config.map(AvccToAnnexBRepacker::new),
                    frame_sender,
                    input_buffer_size: 2000,
                };
                let decoder_result = match options.video_decoders.h264 {
                    VideoDecoderOptions::FfmpegH264 => {
                        VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                            input_id,
                            decoder_thread_options,
                        )
                    }
                    VideoDecoderOptions::VulkanH264 => {
                        VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                            input_id,
                            decoder_thread_options,
                        )
                    }
                    _ => {
                        result_sender
                            .send(Err(InputInitError::InvalidVideoDecoderProvided {
                                expected: VideoCodec::H264,
                            }))
                            .unwrap();
                        return;
                    }
                };
                let handle = match decoder_result {
                    Ok(handle) => handle,
                    Err(err) => {
                        result_sender.send(Err(err.into())).unwrap();
                        return;
                    }
                };

                (Some((stream.index(), handle, state)), Some(frame_receiver))
            }
            None => (None, None),
        };

        result_sender
            .send(Ok(QueueDataReceiver {
                video: frame_receiver.take(),
                audio: samples_receiver.take(),
            }))
            .unwrap();

        let mut pts_offset = Duration::ZERO;
        let start_time = Instant::now();
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

            if let Some((index, ref handle, ref mut state)) = video {
                if packet.stream() == index {
                    let (pts, dts, is_discontinuity) = state.pts_dts_from_packet(&packet);

                    // Some streams give us packets "from the past", which get dropped by the queue
                    // resulting in no video or blinking. This heuristic moves the next packets forward in
                    // time. We only care about video buffer, audio uses the same offset as video
                    // to avoid audio sync issues.
                    if is_discontinuity {
                        pts_offset = Duration::ZERO;
                    } else if handle.chunk_sender.len() < HlsInput::MIN_BUFFER_SIZE
                        && start_time.elapsed() > Duration::from_secs(10)
                    {
                        pts_offset += Duration::from_secs_f64(0.1);
                        warn!(?pts_offset, "Increasing offset");
                    }
                    let pts = pts + pts_offset;

                    let chunk = EncodedInputChunk {
                        data: Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        kind: MediaKind::Video(VideoCodec::H264),
                    };

                    if handle.chunk_sender.is_empty() {
                        debug!("HLS input video channel was drained");
                    }
                    trace!(?chunk, "Sending video chunk");
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

                    let chunk = EncodedInputChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        kind: MediaKind::Audio(AudioCodec::Aac),
                    };

                    if sender.chunk_sender.is_empty() {
                        debug!("HLS input audio channel was drained");
                    }
                    trace!(?chunk, "Sending audio chunk");
                    if sender
                        .chunk_sender
                        .send(PipelineEvent::Data(chunk))
                        .is_err()
                    {
                        debug!("Channel closed")
                    }
                }
            }
        }

        if let Some((_, handle, _)) = audio {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }

        if let Some((_, handle, _)) = video {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct StreamState {
    queue_start_time: Instant,
    buffer_duration: Duration,
    time_base: ffmpeg_next::Rational,

    reference_pts_and_timestamp: Option<(Duration, f64)>,

    pts_discontinuity: DiscontinuityState,
    dts_discontinuity: DiscontinuityState,
}

impl StreamState {
    fn new(
        queue_start_time: Instant,
        time_base: ffmpeg_next::Rational,
        buffer_duration: Duration,
    ) -> Self {
        Self {
            queue_start_time,
            time_base,
            buffer_duration,

            reference_pts_and_timestamp: None,
            pts_discontinuity: DiscontinuityState::new(false, time_base),
            dts_discontinuity: DiscontinuityState::new(true, time_base),
        }
    }

    fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>, bool) {
        let pts_timestamp = packet.pts().unwrap_or(0) as f64;
        let dts_timestamp = packet.dts().map(|dts| dts as f64);
        info!(pts_timestamp, dts_timestamp);
        let packet_duration = packet.duration() as f64;

        let is_pts_discontinuity = self
            .pts_discontinuity
            .detect_discontinuity(pts_timestamp, packet_duration);
        let is_dts_discontinuity = dts_timestamp.is_some_and(|dts| {
            self.dts_discontinuity
                .detect_discontinuity(dts, packet_duration)
        });

        let pts_timestamp = pts_timestamp + self.pts_discontinuity.offset;
        let dts_timestamp = dts_timestamp.map(|dts| dts + self.dts_discontinuity.offset);

        let (reference_pts, reference_timestamp) = *self
            .reference_pts_and_timestamp
            .get_or_insert_with(|| (self.queue_start_time.elapsed(), pts_timestamp));

        let pts_diff_secs = timestamp_to_secs(pts_timestamp - reference_timestamp, self.time_base);
        let pts =
            Duration::from_secs_f64(reference_pts.as_secs_f64() + f64::max(pts_diff_secs, 0.0));

        let dts = dts_timestamp.map(|dts| {
            Duration::from_secs_f64(f64::max(timestamp_to_secs(dts, self.time_base), 0.0))
        });

        (
            pts + self.buffer_duration,
            dts.map(|dts| dts + self.buffer_duration),
            is_pts_discontinuity || is_dts_discontinuity,
        )
    }
}

struct DiscontinuityState {
    check_timestamp_monotonicity: bool,
    time_base: ffmpeg_next::Rational,
    prev_timestamp: Option<f64>,
    next_predicted_timestamp: Option<f64>,
    offset: f64,
}

impl DiscontinuityState {
    /// (10s) This value was picked arbitrarily but it's quite conservative.
    const DISCONTINUITY_THRESHOLD: f64 = 10.0;

    fn new(check_timestamp_monotonicity: bool, time_base: ffmpeg_next::Rational) -> Self {
        Self {
            check_timestamp_monotonicity,
            time_base,
            prev_timestamp: None,
            next_predicted_timestamp: None,
            offset: 0.0,
        }
    }

    fn detect_discontinuity(&mut self, timestamp: f64, packet_duration: f64) -> bool {
        let (Some(prev_timestamp), Some(next_timestamp)) =
            (self.prev_timestamp, self.next_predicted_timestamp)
        else {
            self.prev_timestamp = Some(timestamp);
            self.next_predicted_timestamp = Some(timestamp + packet_duration);
            return false;
        };

        // Detect discontinuity
        let timestamp_delta =
            timestamp_to_secs(f64::abs(next_timestamp - timestamp), self.time_base);

        let mut is_discontinuity = timestamp_delta >= Self::DISCONTINUITY_THRESHOLD
            || (self.check_timestamp_monotonicity && prev_timestamp > timestamp);
        if is_discontinuity {
            debug!("Discontinuity detected: {prev_timestamp} -> {timestamp}");
            self.offset += next_timestamp - timestamp;
            is_discontinuity = true;
        }

        self.prev_timestamp = Some(timestamp);
        self.next_predicted_timestamp = Some(timestamp + packet_duration);

        is_discontinuity
    }
}

fn timestamp_to_secs(timestamp: f64, time_base: ffmpeg_next::Rational) -> f64 {
    f64::max(timestamp, 0.0) * time_base.numerator() as f64 / time_base.denominator() as f64
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

fn read_extra_data(stream: &Stream<'_>) -> Option<Bytes> {
    unsafe {
        let codecpar = (*stream.as_ptr()).codecpar;
        let size = (*codecpar).extradata_size;
        if size > 0 {
            Some(Bytes::copy_from_slice(slice::from_raw_parts(
                (*codecpar).extradata,
                size as usize,
            )))
        } else {
            None
        }
    }
}
