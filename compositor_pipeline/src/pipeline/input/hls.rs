use std::{
    collections::VecDeque,
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
use crossbeam_channel::{bounded, Receiver, Sender};
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
    queue_start_time_sender: Sender<Instant>,
}

impl HlsInput {
    pub(super) fn start_new_input(
        input_id: &InputId,
        opts: HlsInputOptions,
    ) -> Result<InputInitResult, InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let (video, audio, queue_start_time_sender) =
            HlsInputThread::new(input_id.clone(), should_close.clone(), opts).spawn()?;

        Ok(InputInitResult {
            input: Input::Hls(Self {
                should_close,
                queue_start_time_sender,
            }),
            video,
            audio,
            init_info: InputInitInfo::Other,
        })
    }

    pub fn update_queue_start_time(&self, queue_start_time: Instant) {
        if self.queue_start_time_sender.send(queue_start_time).is_err() {
            debug!("HLS input thread already stopped");
        }
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

enum HlsInputState {
    Buffering { chunks: VecDeque<EncodedChunk> },
    Started { queue_start_time: Instant },
}

struct HlsInputThread {
    input_id: InputId,
    should_close: Arc<AtomicBool>,
    options: HlsInputOptions,
    state: HlsInputState,
}

impl HlsInputThread {
    fn new(input_id: InputId, should_close: Arc<AtomicBool>, options: HlsInputOptions) -> Self {
        Self {
            input_id,
            should_close,
            options,
            state: HlsInputState::Buffering {
                chunks: VecDeque::new(),
            },
        }
    }

    fn spawn(
        self,
    ) -> Result<
        (
            Option<VideoInputReceiver>,
            Option<AudioInputReceiver>,
            Sender<Instant>,
        ),
        InputInitError,
    > {
        let (result_sender, result_receiver) = bounded(1);
        let (queue_start_time_sender, queue_start_time_receiver) = bounded(1);
        std::thread::Builder::new()
            .name(format!("HLS thread for input {}", self.input_id.clone()))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "HLS thread",
                    input_id = self.input_id.to_string()
                )
                .entered();

                self.run(queue_start_time_receiver, result_sender);
            })
            .unwrap();

        result_receiver
            .recv()
            .unwrap()
            .map(|(video, audio)| (video, audio, queue_start_time_sender))
    }

    #[allow(clippy::type_complexity)]
    fn run(
        mut self,
        queue_start_time_receiver: Receiver<Instant>,
        result_sender: Sender<
            Result<(Option<VideoInputReceiver>, Option<AudioInputReceiver>), InputInitError>,
        >,
    ) {
        // careful: moving the input context in any way will cause ffmpeg to segfault
        // I do not know why this happens
        let mut input_ctx = match input_with_dictionary_and_interrupt(
            &self.options.url,
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
            || self.should_close.load(Ordering::Relaxed),
        ) {
            Ok(i) => i,
            Err(e) => {
                result_sender
                    .send(Err(InputInitError::FfmpegError(e)))
                    .unwrap();
                return;
            }
        };

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
                let (sender, receiver) = ChunkSender::new(2000);
                let discontinuity_state = DiscontinuityState::new(stream.time_base());
                (
                    Some((
                        stream.index(),
                        stream.time_base(),
                        sender,
                        discontinuity_state,
                    )),
                    Some((receiver, config)),
                )
            }
            None => (None, None),
        };
        let (mut video, video_receiver) = match input_ctx.streams().best(Type::Video) {
            Some(stream) => {
                let (sender, receiver) = ChunkSender::new(2000);
                let discontinuity_state = DiscontinuityState::new(stream.time_base());
                (
                    Some((
                        stream.index(),
                        stream.time_base(),
                        sender,
                        discontinuity_state,
                    )),
                    Some(receiver),
                )
            }
            None => (None, None),
        };

        result_sender
            .send(Ok((
                video_receiver.map(|video| VideoInputReceiver::Encoded {
                    chunk_receiver: video,
                    decoder_options: self.options.video_decoder,
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

        loop {
            // TODO(noituri):  Move it outside
            if let Ok(queue_start_time) = queue_start_time_receiver.try_recv() {
                let prev_state =
                    std::mem::replace(&mut self.state, HlsInputState::Started { queue_start_time });
                match prev_state {
                    HlsInputState::Buffering { chunks } => {
                        tracing::error!("Received correct queue start time {queue_start_time:?}");
                        // TODO(noituri):  Recalculate pts
                        for chunk in chunks {
                            match chunk.kind {
                                EncodedChunkKind::Video(_) => {
                                    if let Some((_, _, ref sender, _)) = video {
                                        if sender.send(chunk, queue_start_time).is_err() {
                                            debug!("Channel closed")
                                        }
                                    }
                                }
                                EncodedChunkKind::Audio(_) => {
                                    if let Some((_, _, ref sender, _)) = audio {
                                        if sender.send(chunk, queue_start_time).is_err() {
                                            debug!("Channel closed")
                                        }
                                    }
                                }
                            }
                        }
                    }
                    HlsInputState::Started { .. } => {
                        warn!("Trying to start already started hls input");
                    }
                }
            }

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

            if let Some((index, time_base, ref sender, ref mut discontinuity)) = video {
                discontinuity.update(&packet);
                let pts = discontinuity.recalculate_pts(packet.pts());
                let dts = discontinuity.recalculate_dts(packet.dts());

                if packet.stream() == index {
                    let chunk = EncodedChunk {
                        data: Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Video(VideoCodec::H264),
                    };

                    match &mut self.state {
                        HlsInputState::Buffering { chunks } => chunks.push_back(chunk),
                        HlsInputState::Started { queue_start_time } => {
                            if sender.is_empty() {
                                warn!("HLS input video channel was drained")
                            }
                            if sender.send(PipelineEvent::Data(chunk, queue_start_time)).is_err() {
                                debug!("Channel closed")
                            }
                        }
                    }
                }
            }

            if let Some((index, time_base, ref sender, ref mut discontinuity)) = audio {
                discontinuity.update(&packet);
                let pts = discontinuity.recalculate_pts(packet.pts());
                let dts = discontinuity.recalculate_dts(packet.dts());

                if packet.stream() == index {
                    let chunk = EncodedChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts,
                        dts,
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Audio(AudioCodec::Aac),
                    };

                    match &mut self.state {
                        HlsInputState::Buffering { chunks } => chunks.push_back(chunk),
                        HlsInputState::Started { queue_start_time } => {
                            if sender.is_empty() {
                                warn!("HLS input video channel was drained")
                            }
                            if sender.send(PipelineEvent::Data(chunk), queue_start_time).is_err() {
                                debug!("Channel closed")
                            }
                        }
                    }
                }
            }
        }

        if let Some((_, _, sender, _)) = audio {
            sender.send_eos();
        }

        if let Some((_, _, sender, _)) = video {
            sender.send_eos();
        }
    }
}

struct ChunkSender {
    sender: Sender<PipelineEvent<EncodedChunk>>,
}

impl ChunkSender {
    fn new(cap: usize) -> (Self, Receiver<PipelineEvent<EncodedChunk>>) {
        let (sender, receiver) = bounded(cap);
        let sender = Self { sender };

        (sender, receiver)
    }

    fn send(&self, chunk: EncodedChunk, queue_start_time: Instant) {
        if self.sender.is_empty() {
            warn!("HLS input video channel was drained")
        }
        if self.sender.send(PipelineEvent::Data(chunk)).is_err() {
            debug!("Channel closed")
        }
    }

    fn send_eos(&self) {
        if sender.send(PipelineEvent::EOS).is_err() {
            debug!("Failed to send EOS message.")
        }
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

// TODO(noituri): Write tests
// TODO(noituri): Test on long run
struct DiscontinuityState {
    prev_dts: Option<f64>,
    offset: f64,
    // TODO(noituri): time base can change?
    time_base: ffmpeg_next::Rational,
}

impl DiscontinuityState {
    /// (10s) This value was picked by ffmpeg arbitrarily but it's quite conservative.
    const DISCONTINUITY_THRESHOLD: f64 = 10_000.0;

    fn new(time_base: ffmpeg_next::Rational) -> Self {
        Self {
            prev_dts: None,
            offset: 0.0,
            time_base,
        }
    }

    fn update(&mut self, packet: &Packet) {
        let dts = packet.dts().unwrap_or(0) as f64;
        let prev_dts = self.prev_dts.unwrap_or(dts);
        let to_timestamp = self.time_base.numerator() as f64 / self.time_base.denominator() as f64;
        if f64::abs((dts + self.offset) - prev_dts) * to_timestamp * 1000.0
            >= Self::DISCONTINUITY_THRESHOLD
        {
            // TODO(noituri): Use debug here
            tracing::error!("Discontinuity detected: {prev_dts} -> {dts} (dts)");
            self.offset = (prev_dts - dts) + packet.duration() as f64;
        }

        self.prev_dts = Some(dts + self.offset);
    }

    fn recalculate_dts(&self, dts: Option<i64>) -> Option<Duration> {
        dts.map(|dts| {
            Duration::from_secs_f64(
                (dts as f64 + self.offset) * self.time_base.numerator() as f64
                    / self.time_base.denominator() as f64,
            )
        })
    }

    fn recalculate_pts(&self, pts: Option<i64>) -> Duration {
        Duration::from_secs_f64(
            (pts.unwrap_or(0) as f64 + self.offset) * self.time_base.numerator() as f64
                / self.time_base.denominator() as f64,
        )
    }
}
