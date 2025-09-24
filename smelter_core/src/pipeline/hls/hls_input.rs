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
use crossbeam_channel::{bounded, Receiver};
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
use smelter_render::InputId;
use tracing::{debug, error, span, trace, warn, Level};

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264,
            h264_utils::{AvccToAnnexBRepacker, H264AvcDecoderConfig},
            vulkan_h264, DecoderThreadHandle,
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

struct Track {
    index: usize,
    handle: DecoderThreadHandle,
    state: StreamState,
}

impl HlsInput {
    const MIN_BUFFER_SIZE: Duration = Duration::from_secs(1);

    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: HlsInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let buffer_duration = opts.buffer_duration.unwrap_or(Duration::from_secs(2));

        let input_ctx = FfmpegInputContext::new(&opts.url, should_close.clone())?;
        let (audio, samples_receiver) = match input_ctx.audio_stream() {
            Some(stream) => {
                let (track, receiver) =
                    Self::handle_audio_track(&ctx, &input_id, &stream, buffer_duration)?;
                (Some(track), Some(receiver))
            }
            None => (None, None),
        };
        let (video, frame_receiver) = match input_ctx.video_stream() {
            Some(stream) => {
                let (track, receiver) = Self::handle_video_track(
                    &ctx,
                    &input_id,
                    &stream,
                    opts.video_decoders,
                    buffer_duration,
                )?;
                (Some(track), Some(receiver))
            }
            None => (None, None),
        };

        let receivers = QueueDataReceiver {
            video: frame_receiver,
            audio: samples_receiver,
        };

        Self::spawn_demuxer_thread(input_id, input_ctx, audio, video);

        Ok((
            Input::Hls(Self { should_close }),
            InputInitInfo::Other,
            receivers,
        ))
    }

    fn handle_audio_track(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
        stream: &Stream<'_>,
        buffer_duration: Duration,
    ) -> Result<(Track, Receiver<PipelineEvent<InputAudioSamples>>), InputInitError> {
        // not tested it was always null, but audio is in ADTS, so config is not
        // necessary
        let asc = read_extra_data(stream);
        let (samples_sender, samples_receiver) = bounded(5);
        let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer_duration);
        let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
            input_id.clone(),
            AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: FdkAacDecoderOptions { asc },
                samples_sender,
                input_buffer_size: 2000,
            },
        )?;

        Ok((
            Track {
                index: stream.index(),
                handle,
                state,
            },
            samples_receiver,
        ))
    }

    fn handle_video_track(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
        stream: &Stream<'_>,
        video_decoders: HlsInputVideoDecoders,
        buffer_duration: Duration,
    ) -> Result<(Track, Receiver<PipelineEvent<Frame>>), InputInitError> {
        let (frame_sender, frame_receiver) = bounded(5);
        let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer_duration);

        let extra_data = read_extra_data(stream);
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

        let vulkan_supported = ctx.graphics_context.has_vulkan_support();
        let h264_decoder = video_decoders.h264.unwrap_or({
            match vulkan_supported {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        });

        let handle = match h264_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                    input_id,
                    decoder_thread_options,
                )?
            }
            VideoDecoderOptions::VulkanH264 => {
                if !vulkan_supported {
                    return Err(InputInitError::DecoderError(
                        DecoderInitError::VulkanContextRequiredForVulkanDecoder,
                    ));
                }
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                    input_id,
                    decoder_thread_options,
                )?
            }
            _ => {
                return Err(InputInitError::InvalidVideoDecoderProvided {
                    expected: VideoCodec::H264,
                });
            }
        };

        Ok((
            Track {
                index: stream.index(),
                handle,
                state,
            },
            frame_receiver,
        ))
    }

    fn spawn_demuxer_thread(
        input_id: InputId,
        input_ctx: FfmpegInputContext,
        audio: Option<Track>,
        video: Option<Track>,
    ) {
        std::thread::Builder::new()
            .name(format!("HLS thread for input {}", input_id.clone()))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "HLS thread", input_id = input_id.to_string()).entered();

                Self::run_demuxer_thread(input_ctx, audio, video);
            })
            .unwrap();
    }

    fn run_demuxer_thread(
        mut input_ctx: FfmpegInputContext,
        mut audio: Option<Track>,
        mut video: Option<Track>,
    ) {
        let mut pts_offset = Duration::ZERO;
        loop {
            let packet = match input_ctx.read_packet() {
                Ok(packet) => packet,
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(err) => {
                    warn!("HLS read error {err:?}");
                    continue;
                }
            };

            if packet.is_corrupt() {
                error!(
                    "Corrupted packet {:?} {:?}",
                    packet.stream(),
                    packet.flags()
                );
                continue;
            }

            if let Some(track) = &mut video {
                if packet.stream() == track.index {
                    let (pts, dts, is_discontinuity) = track.state.pts_dts_from_packet(&packet);

                    // Some streams give us packets "from the past", which get dropped by the queue
                    // resulting in no video or blinking. This heuristic moves the next packets forward in
                    // time. We only care about video buffer, audio uses the same offset as video
                    // to avoid audio sync issues.
                    let min_pts =
                        track.state.queue_start_time.elapsed() + HlsInput::MIN_BUFFER_SIZE;
                    if is_discontinuity {
                        pts_offset = Duration::ZERO;
                    } else if min_pts > pts + pts_offset {
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

                    let sender = &track.handle.chunk_sender;
                    trace!(?chunk, "Sending video chunk");
                    if sender.is_empty() {
                        debug!("HLS input video channel was drained");
                    }
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Channel closed")
                    }
                }
            }

            if let Some(track) = &mut audio {
                if packet.stream() == track.index {
                    let (pts, dts, _) = track.state.pts_dts_from_packet(&packet);
                    let pts = pts + pts_offset;

                    let chunk = EncodedInputChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        kind: MediaKind::Audio(AudioCodec::Aac),
                    };

                    let sender = &track.handle.chunk_sender;
                    trace!(?chunk, "Sending audio chunk");
                    if sender.is_empty() {
                        debug!("HLS input audio channel was drained");
                    }
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Channel closed")
                    }
                }
            }
        }

        if let Some(Track { handle, .. }) = &audio {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Channel closed. Failed to send audio EOS.")
            }
        }

        if let Some(Track { handle, .. }) = &video {
            if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Channel closed. Failed to send video EOS.")
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

struct FfmpegInputContext {
    ctx: context::Input,
}

impl FfmpegInputContext {
    fn new(url: &Arc<str>, should_close: Arc<AtomicBool>) -> Result<Self, ffmpeg_next::Error> {
        let ctx = input_with_dictionary_and_interrupt(
            url,
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
            // move is required even though types do not require it
            move || should_close.load(Ordering::Relaxed),
        )?;
        Ok(Self { ctx })
    }

    fn audio_stream(&self) -> Option<Stream<'_>> {
        self.ctx.streams().best(Type::Audio)
    }

    fn video_stream(&self) -> Option<Stream<'_>> {
        self.ctx.streams().best(Type::Video)
    }

    fn read_packet(&mut self) -> Result<Packet, ffmpeg_next::Error> {
        let mut packet = Packet::empty();
        packet.read(&mut self.ctx)?;
        Ok(packet)
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
    F: FnMut() -> bool + 'static,
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
