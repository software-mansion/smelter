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
        decoder::{self, AacDecoderOptions, AudioDecoderOptions},
        types::{EncodedChunk, IsKeyframe},
        AudioCodec, EncodedChunkKind, VideoCodec,
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
use tracing::{debug, span, warn, Level};

use super::{AudioInputReceiver, Input, InputInitInfo, InputInitResult, VideoInputReceiver};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoder: decoder::VideoDecoderOptions,
}

pub struct HlsInput {
    should_close: Arc<AtomicBool>,
}

impl HlsInput {
    pub(super) fn start_new_input(
        input_id: &InputId,
        queue_start_time: Option<Instant>,
        opts: HlsInputOptions,
    ) -> Result<InputInitResult, InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let (video, audio) = Self::spawn_thread(
            input_id.clone(),
            should_close.clone(),
            queue_start_time,
            opts,
        )?;

        Ok(InputInitResult {
            input: Input::Hls(Self { should_close }),
            video,
            audio,
            init_info: InputInitInfo::Other,
        })
    }

    fn spawn_thread(
        input_id: InputId,
        should_close: Arc<AtomicBool>,
        queue_start_time: Option<Instant>,
        options: HlsInputOptions,
    ) -> Result<(Option<VideoInputReceiver>, Option<AudioInputReceiver>), InputInitError> {
        let (result_sender, result_receiver) = bounded(1);
        std::thread::Builder::new()
            .name(format!("HLS thread for input {}", input_id.clone()))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "HLS thread", input_id = input_id.to_string()).entered();

                Self::run_thread(options, should_close, queue_start_time, result_sender);
            })
            .unwrap();

        result_receiver.recv().unwrap()
    }

    #[allow(clippy::type_complexity)]
    fn run_thread(
        options: HlsInputOptions,
        should_close: Arc<AtomicBool>,
        queue_start_time: Option<Instant>,
        result_sender: Sender<
            Result<(Option<VideoInputReceiver>, Option<AudioInputReceiver>), InputInitError>,
        >,
    ) {
        // careful: moving the input context in any way will cause ffmpeg to segfault
        // I do not know why this happens
        let mut input_ctx = match input_with_dictionary_and_interrupt(
            &options.url,
            Dictionary::from_iter([
                ("protocol_whitelist", "tcp,hls,http,https,file,tls"),
                // ("rtbufsize", "1000M"),
                // ("fflags", "nobuffer"),
                // ("flags", "low_delay"),
            ]),
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
        let queue_start_time = queue_start_time.unwrap_or(Instant::now());

        let (mut audio, audio_result) = match input_ctx.streams().best(Type::Audio) {
            Some(stream) => {
                // not tested it was always null, but audio is in ADTS, so config is not
                // necessary
                let config = unsafe {
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
                let (sender, receiver) = bounded(2000);
                let timestamp_state =
                    TimestampState::new(input_start_time, queue_start_time, stream.time_base());
                (
                    Some((stream.index(), sender, timestamp_state)),
                    Some((receiver, config)),
                )
            }
            None => (None, None),
        };
        let (mut video, video_receiver) = match input_ctx.streams().best(Type::Video) {
            Some(stream) => {
                // TODO(noituri): Decoder sometimes in rare cases might need extradata
                let (sender, receiver) = bounded(2000);
                let timestamp_state =
                    TimestampState::new(input_start_time, queue_start_time, stream.time_base());
                (
                    Some((stream.index(), sender, timestamp_state)),
                    Some(receiver),
                )
            }
            None => (None, None),
        };

        let mut send_result = Some(move || {
            result_sender
                .send(Ok((
                    video_receiver.map(|video| VideoInputReceiver::Encoded {
                        chunk_receiver: video,
                        decoder_options: options.video_decoder,
                    }),
                    audio_result.map(|(receiver, asc)| AudioInputReceiver::Encoded {
                        chunk_receiver: receiver,
                        decoder_options: AudioDecoderOptions::Aac(AacDecoderOptions {
                            depayloader_mode: None,
                            asc,
                        }),
                    }),
                )))
                .unwrap();
        });

        let mut drain_start = None;
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

            if packet.flags().contains(ffmpeg_next::packet::Flags::CORRUPT) {
                debug!(
                    "Corrupted packet {:?} {:?}",
                    packet.stream(),
                    packet.flags()
                );
                continue;
            }

            if let Some((index, ref sender, ref mut timestamp_state)) = video {
                let buffer_len = match &send_result {
                    Some(_) => None,
                    None => Some(sender.len()),
                };
                timestamp_state.update(&packet, buffer_len);
                let (pts, dts) = timestamp_state.pts_dts_from_packet(&packet);

                if packet.stream() == index {
                    let chunk = EncodedChunk {
                        data: Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Video(VideoCodec::H264),
                    };

                    if sender.is_empty() {
                        if drain_start.is_none() {
                            drain_start = Some(Instant::now());
                        }
                        dbg!((
                            sender.len(),
                            timestamp_state.timestamp_offset,
                            packet.duration()
                        ));
                        warn!("HLS input video channel was drained")
                    } else if let Some(drain_start) = drain_start.take() {
                        tracing::error!("No packets for {:?}", drain_start.elapsed());
                    }
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Channel closed")
                    }
                }
            }

            if let Some((index, ref sender, ref mut timestamp_state)) = audio {
                let buffer_len = match &send_result {
                    Some(_) => None,
                    None => Some(sender.len()),
                };
                timestamp_state.update(&packet, buffer_len);
                let (pts, dts) = timestamp_state.pts_dts_from_packet(&packet);

                if packet.stream() == index {
                    let chunk = EncodedChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Audio(AudioCodec::Aac),
                    };

                    if sender.is_empty() {
                        warn!("HLS input audio channel was drained")
                    }
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Channel closed")
                    }
                }
            }

            if send_result.is_some() {
                match (&video, &audio) {
                    (Some((_, ref video, _)), Some((_, ref audio, _))) => {
                        if video.len() > TimestampState::BUFFER_SIZE
                            && audio.len() > TimestampState::BUFFER_SIZE
                        {
                            let send_result = send_result.take().unwrap();
                            send_result();
                        }
                    }
                    (Some((_, ref video, _)), None) => {
                        if video.len() > TimestampState::BUFFER_SIZE {
                            let send_result = send_result.take().unwrap();
                            send_result();
                        }
                    }
                    (_, Some((_, ref audio, _))) => {
                        if audio.len() > TimestampState::BUFFER_SIZE {
                            let send_result = send_result.take().unwrap();
                            send_result();
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some((_, sender, _)) = audio {
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }

        if let Some((_, sender, _)) = video {
            if sender.send(PipelineEvent::EOS).is_err() {
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

// TODO(noituri): Test on long run
struct TimestampState {
    input_start_time: Instant,
    queue_start_time: Instant,
    first_pts: Option<Duration>,

    prev_pts: Option<f64>,
    timestamp_offset: f64,
    discontinuity_offset: f64,
    time_base: ffmpeg_next::Rational,
}

impl TimestampState {
    /// (10s) This value was picked arbitrarily but it's quite conservative.
    const DISCONTINUITY_THRESHOLD: f64 = 10.0;
    const BUFFER_SIZE_DELTA_TRESHOLD: f64 = 20.0;
    const BUFFER_SIZE: usize = 50;
    const ADAPTIVE_TIMESTAMP_INCREMENT: f64 = 0.005;

    fn new(
        input_start_time: Instant,
        queue_start_time: Instant,
        time_base: ffmpeg_next::Rational,
    ) -> Self {
        Self {
            input_start_time,
            queue_start_time,
            first_pts: None,
            prev_pts: None,
            discontinuity_offset: 0.0,
            timestamp_offset: 0.0,
            time_base,
        }
    }

    fn update(&mut self, packet: &Packet, buffer_len: Option<usize>) {
        let pts = packet.pts().unwrap_or(0) as f64;
        let prev_pts = self.prev_pts.unwrap_or(pts);

        // Detect discontinuity
        let timestamp_delta = self
            .to_timestamp(f64::abs(
                pts + self.timestamp_offset + self.discontinuity_offset - prev_pts,
            ))
            .as_secs_f64();
        if timestamp_delta >= Self::DISCONTINUITY_THRESHOLD {
            tracing::error!("Discontinuity detected: {prev_pts} -> {pts} (pts)");
            self.discontinuity_offset = (prev_pts - pts) + packet.duration() as f64;
        }

        self.prev_pts = Some(pts + self.timestamp_offset + self.discontinuity_offset);

        if let Some(buffer_len) = buffer_len {
            let buffer_delta = f64::max(0.0, Self::BUFFER_SIZE as f64 - buffer_len as f64);
            if buffer_delta > Self::BUFFER_SIZE_DELTA_TRESHOLD {
                self.timestamp_offset += Self::ADAPTIVE_TIMESTAMP_INCREMENT
                    * self.time_base.denominator() as f64
                    / self.time_base.numerator() as f64;
            }
        }
    }

    fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>) {
        let pts = self.to_timestamp(
            packet.pts().unwrap_or(0) as f64 as f64
                + self.timestamp_offset
                + self.discontinuity_offset,
        );
        let dts = packet.dts().map(|dts| {
            self.to_timestamp(dts as f64 + self.timestamp_offset + self.discontinuity_offset)
        });

        // Recalculate pts in regards to queue start time
        let first_pts = *self.first_pts.get_or_insert(pts);
        let pts = self.to_queue_timestamp(pts.saturating_sub(first_pts));

        return (pts, dts);
    }

    fn to_timestamp(&self, timestamp: f64) -> Duration {
        Duration::from_secs_f64(
            timestamp * self.time_base.numerator() as f64 / self.time_base.denominator() as f64,
        )
    }

    fn to_queue_timestamp(&self, input_timestamp: Duration) -> Duration {
        (self.input_start_time + input_timestamp).duration_since(self.queue_start_time)
    }
}
