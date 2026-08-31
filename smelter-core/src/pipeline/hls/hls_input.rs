use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::Bytes;
use ffmpeg_next::{
    Dictionary, Packet, Rational,
    format::{context, input_with_interrupt_and_dictionary},
    media::Type,
};
use tracing::{Level, Span, debug, info, span, trace, warn};

use crate::{
    pipeline::{
        decoder::{DecoderThreadHandle, EncodedInputEvent},
        ffmpeg_utils::ReadExtradataExt,
        input::Input,
        utils::{
            channel::TrySendError,
            input_sync::{
                InputSync, InputSyncItem, InputSyncTrack, SimpleSync, TimestampAnchor,
                TrackClosedError, TrackEvent, TrackKind, TrackSink,
            },
            live_sync::{BufferingStrategy, FifoBuffer, LiveSync, LiveSyncOptions},
        },
    },
    queue::{QueueInput, QueueSender, QueueTrackOffset, QueueTrackOptions},
};

use super::hls_decoder::{spawn_audio_decoder, spawn_video_decoder};

use crate::prelude::*;

type HlsBuffer = FifoBuffer<HlsPacket>;

/// HLS input - reads from an HLS URL via FFmpeg, demuxes H.264/AAC tracks,
/// decodes, and feeds frames/samples into the queue.
///
/// ## Timestamps
///
/// - FFmpeg opens the HLS URL immediately and discovers tracks.
/// - Whether the playlist is live decides which synchronization is used. FFmpeg
///   only knows the duration of a playlist that ended (`#EXT-X-ENDLIST`), so a
///   playlist without a duration is a live one.
/// - Live playlist (`InputSync::Live`)
///   - Packet timestamps are passed through as they are; live sync maps them
///     onto the timeline of the queue sync point and keeps the buffer in range.
///   - Register track with `QueueTrackOffset::Pts(Duration::ZERO)`
///   - `offset` is ignored, the live edge decides where the playback starts.
/// - Non-live playlist (`InputSync::Simple`)
///   - Timestamps are normalized, so PTS of the first packet is zero.
///   - Register track with `QueueTrackOffset::FromStart(offset)`, or with
///     `QueueTrackOffset::None` if the offset is not defined.
/// - On discontinuity (detected by the sync, only for live playlists)
///   - Send `EncodedInputEvent::Discontinuity` to the decoder of the track
///     that broke, so it decodes what it still holds from the old timeline
///     and then drops the state built from it
///   - Ignore packets until `packet.key() == true`
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
pub struct HlsInput {
    should_close: Arc<AtomicBool>,
}

impl HlsInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: HlsInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let _span = span!(Level::INFO, "HLS input", input_id = input_ref.to_string()).entered();
        let should_close = Arc::new(AtomicBool::new(false));
        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Hls,
        });

        let ffmpeg_ctx = FfmpegInputContext::new(&opts.url, should_close.clone())?;
        let queue_input = QueueInput::new(&ctx, &input_ref, opts.queue_options.clone());

        let input_ctx = HlsInputContext {
            ctx,
            input_ref,
            opts,
        };

        HlsDemuxerThread::new(input_ctx, ffmpeg_ctx, &queue_input)?.spawn();

        Ok((
            Input::Hls(Self { should_close }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// What the tracks of one input share: where they belong in the pipeline and
/// how to decode them.
pub(super) struct HlsInputContext {
    pub(super) ctx: Arc<PipelineCtx>,
    pub(super) input_ref: Ref<InputId>,
    pub(super) opts: HlsInputOptions,
}

struct HlsDemuxerThread {
    ffmpeg_ctx: FfmpegInputContext,
    input_sync: InputSync<HlsBuffer>,

    audio: Option<BufferTrackWriter>,
    video: Option<BufferTrackWriter>,
}

impl HlsDemuxerThread {
    fn new(
        input_ctx: HlsInputContext,
        ffmpeg_ctx: FfmpegInputContext,
        queue_input: &QueueInput,
    ) -> Result<Self, InputInitError> {
        let offset = input_ctx.opts.offset;
        let is_live = ffmpeg_ctx.is_live();
        let audio_stream = ffmpeg_ctx.audio_stream();
        let video_stream = ffmpeg_ctx.video_stream();

        info!(
            is_live,
            has_video = video_stream.is_some(),
            has_audio = audio_stream.is_some(),
            "HLS playlist opened"
        );
        if is_live && offset.is_some() {
            warn!("Offset is ignored for live playlists, playback starts at the live edge");
        }

        let (frame_sender, samples_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: video_stream.is_some(),
            audio: audio_stream.is_some(),
            offset: match is_live {
                true => QueueTrackOffset::Pts(Duration::ZERO),
                false => match offset {
                    Some(offset) => QueueTrackOffset::FromStart(offset),
                    None => QueueTrackOffset::None,
                },
            },
        });

        let buffer_opts = input_ctx.opts.buffer;
        let (min, desired, max) = resolve_buffer_options(buffer_opts);
        let input_sync = match is_live {
            true => InputSync::Live(LiveSync::new(
                LiveSyncOptions {
                    buffering_strategy: BufferingStrategy::Range { min, max, desired },
                    stabilization_period: Duration::from_millis(1000),
                    // mostly dead value, chunks are usually larger than stabilization_period
                    // so tolerance does not matter, stable state will trigger as soon as we have
                    // a gap between chunks.
                    stabilization_tolerance: Duration::from_millis(250),
                    max_wait: Duration::from_secs(10),
                },
                input_ctx.ctx.queue_ctx.sync_point,
            )),
            false => InputSync::Simple(SimpleSync::new(
                buffer_opts.desired.unwrap_or(NON_LIVE_DEFAULT_BUFFER),
            )),
        };
        let decoder_buffer_size = Duration::max(Duration::from_secs(60), max * 2);

        let audio = match (audio_stream, samples_sender) {
            (Some(stream), Some(sender)) => Some(stream.start_audio_track(
                &input_ctx,
                &input_sync,
                sender,
                decoder_buffer_size,
            )?),
            _ => None,
        };
        let video = match (video_stream, frame_sender) {
            (Some(stream), Some(sender)) => Some(stream.start_video_track(
                &input_ctx,
                &input_sync,
                sender,
                decoder_buffer_size,
            )?),
            _ => None,
        };

        Ok(Self {
            ffmpeg_ctx,
            input_sync,
            audio,
            video,
        })
    }

    fn spawn(mut self) {
        let span = Span::current();
        std::thread::Builder::new()
            .name("HLS demuxer thread".to_string())
            .spawn(move || {
                let _span = span.enter();
                self.run();
                info!("Playlist finished")
            })
            .unwrap();
    }

    fn run(&mut self) {
        loop {
            let packet = match self.ffmpeg_ctx.read_packet() {
                Ok(packet) => packet,
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(err) => {
                    warn!("HLS read error {err:?}");
                    continue;
                }
            };
            let stream_id = packet.stream();

            let Some(track) = self.track(stream_id) else {
                trace!(stream_id, "Unknown stream");
                continue;
            };

            if packet.is_corrupt() {
                warn!(stream_id, "Dropping corrupted packet");
                continue;
            }

            if track.write_packet(packet).is_err() {
                break;
            }
        }

        self.input_sync.flush();
    }

    fn track(&mut self, stream_id: usize) -> Option<&mut BufferTrackWriter> {
        [self.video.as_mut(), self.audio.as_mut()]
            .into_iter()
            .flatten()
            .find(|track| track.index == stream_id)
    }
}

/// One track of the playlist, as the demuxer described it.
struct HlsStream {
    index: usize,
    time_base: Rational,
    extradata: Option<Bytes>,
}

impl HlsStream {
    fn start_audio_track(
        self,
        input_ctx: &HlsInputContext,
        input_sync: &InputSync<HlsBuffer>,
        samples_sender: QueueSender<InputAudioSamples>,
        decoder_buffer_size: Duration,
    ) -> Result<BufferTrackWriter, InputInitError> {
        let stats_sender = HlsInputTrackStatsSender::new(input_ctx, TrackKind::Audio);
        let decoder_handle = spawn_audio_decoder(
            input_ctx,
            self.extradata,
            samples_sender,
            decoder_buffer_size,
        )?;
        let track_sync = input_sync.add_track(
            TrackKind::Audio,
            Box::new(DecoderTrackWriter::new(
                input_ctx,
                decoder_handle,
                MediaKind::Audio(AudioCodec::Aac),
                stats_sender.clone(),
                input_sync.is_live(),
            )),
        );

        Ok(BufferTrackWriter {
            index: self.index,
            time_base: self.time_base,
            track_sync,
            stats_sender,
        })
    }

    fn start_video_track(
        self,
        input_ctx: &HlsInputContext,
        input_sync: &InputSync<HlsBuffer>,
        frame_sender: QueueSender<Frame>,
        decoder_buffer_size: Duration,
    ) -> Result<BufferTrackWriter, InputInitError> {
        let stats_sender = HlsInputTrackStatsSender::new(input_ctx, TrackKind::Video);
        let decoder_handle =
            spawn_video_decoder(input_ctx, self.extradata, frame_sender, decoder_buffer_size)?;
        let track_sync = input_sync.add_track(
            TrackKind::Video,
            Box::new(DecoderTrackWriter::new(
                input_ctx,
                decoder_handle,
                MediaKind::Video(VideoCodec::H264),
                stats_sender.clone(),
                input_sync.is_live(),
            )),
        );

        Ok(BufferTrackWriter {
            index: self.index,
            time_base: self.time_base,
            track_sync,
            stats_sender,
        })
    }
}

struct BufferTrackWriter {
    index: usize,
    time_base: Rational,
    track_sync: InputSyncTrack<HlsBuffer>,
    stats_sender: HlsInputTrackStatsSender,
}

impl BufferTrackWriter {
    /// Fails once the consumer of the track is gone.
    fn write_packet(&mut self, packet: Packet) -> Result<(), TrackClosedError> {
        self.stats_sender.send_on_packet_received(&packet);

        trace!(
            stream_id = self.index,
            pts = packet.pts(),
            key = packet.is_key(),
            "Received packet"
        );
        self.track_sync
            .write_chunk(HlsPacket::new(packet, self.time_base))
    }
}

/// Demuxed packet of one track, buffered by the sync as it was read.
/// Timestamps are calculated only when they are needed, so a packet that
/// never leaves the sync costs nothing to convert.
struct HlsPacket {
    packet: Packet,
    time_base: Rational,
    /// Mapping onto the output timeline; identity until the track applies its
    /// own on read.
    anchor: TimestampAnchor,
}

impl HlsPacket {
    fn new(packet: Packet, time_base: Rational) -> Self {
        Self {
            packet,
            time_base,
            anchor: TimestampAnchor {
                input_pts: Duration::ZERO,
                output_pts: Duration::ZERO,
            },
        }
    }

    fn is_key(&self) -> bool {
        self.packet.is_key()
    }

    /// Timestamp of this packet on the output timeline.
    fn timestamp(&self, timestamp: i64) -> Duration {
        let timestamp = Duration::from_secs_f64(
            f64::max(timestamp as f64, 0.0) * self.time_base.numerator() as f64
                / self.time_base.denominator() as f64,
        );
        self.anchor.to_output_pts(timestamp)
    }

    fn into_chunk(self, kind: MediaKind) -> EncodedInputChunk {
        EncodedInputChunk {
            data: Bytes::copy_from_slice(self.packet.data().unwrap_or_default()),
            pts: self.pts(),
            dts: self.packet.dts().map(|dts| self.timestamp(dts)),
            kind,
            present: true,
        }
    }
}

impl InputSyncItem for HlsPacket {
    fn pts(&self) -> Duration {
        self.timestamp(self.packet.pts().unwrap_or(0))
    }

    fn apply_anchor(&mut self, anchor: TimestampAnchor) {
        self.anchor = anchor;
    }
}

/// Consumer of one HLS track: turns the packets the track releases into
/// chunks for its decoder thread and reports the buffer measured at that
/// moment.
struct DecoderTrackWriter {
    decoder_handle: DecoderThreadHandle,
    kind: MediaKind,
    stats_sender: HlsInputTrackStatsSender,
    waiting_for_keyframe: bool,
    pending_discontinuity: bool,
    closed: bool,

    is_live: bool,

    // Used to calculate stats
    sync_point: Instant,
}

impl DecoderTrackWriter {
    fn new(
        input: &HlsInputContext,
        decoder_handle: DecoderThreadHandle,
        kind: MediaKind,
        stats_sender: HlsInputTrackStatsSender,
        is_live: bool,
    ) -> Self {
        Self {
            kind,
            decoder_handle,
            stats_sender,
            sync_point: input.ctx.queue_ctx.sync_point,
            is_live,
            waiting_for_keyframe: true,
            pending_discontinuity: false,
            closed: false,
        }
    }

    /// The input timeline broke. The decoder keeps running - it decodes
    /// everything it still holds from the old timeline first, and drops the
    /// state built from it when the marker reaches it.
    fn on_discontinuity(&mut self) {
        self.stats_sender
            .send(HlsInputTrackStatsEvent::DiscontinuityDetected);
        self.pending_discontinuity = true;
        self.waiting_for_keyframe = true;
        self.maybe_handle_discontinuity();
    }

    fn send_chunk(&mut self, packet: HlsPacket) {
        self.maybe_handle_discontinuity();
        if self.pending_discontinuity {
            // drop `packet` because discontinuity still not handled
            return;
        }

        if self.waiting_for_keyframe {
            if !packet.is_key() {
                debug!("Waiting for keyframe");
                return;
            }
            self.waiting_for_keyframe = false;
        }

        let chunk = packet.into_chunk(self.kind);
        self.stats_sender.send_on_chunk_released(
            self.decoder_handle.chunk_sender.buffered_duration(),
            // only live timestamps are on the sync point timeline
            self.is_live
                .then(|| chunk.pts.saturating_sub(self.sync_point.elapsed())),
        );

        self.send_to_decoder(EncodedInputEvent::Chunk(chunk));
    }

    fn maybe_handle_discontinuity(&mut self) {
        if !self.pending_discontinuity {
            return;
        }
        self.pending_discontinuity = !self.send_to_decoder(EncodedInputEvent::Discontinuity);
    }

    /// Pushes an event to the decoder thread; `false` when it did not fit
    /// (live only) or the decoder is gone. A live stream cannot wait for a
    /// decoder that is not keeping up, a non-live one waits for room.
    fn send_to_decoder(&mut self, event: EncodedInputEvent) -> bool {
        let event = PipelineEvent::Data(event);
        match self.is_live {
            true => match self.decoder_handle.chunk_sender.try_send(event) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    debug!("Dropping chunk; decoder is not keeping up");
                    self.waiting_for_keyframe = true;
                    false
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.closed = true;
                    false
                }
            },
            false => match self.decoder_handle.chunk_sender.send(event) {
                Ok(()) => true,
                Err(_) => {
                    self.closed = true;
                    false
                }
            },
        }
    }
}

impl TrackSink<HlsPacket> for DecoderTrackWriter {
    fn on_event(&mut self, event: TrackEvent<HlsPacket>) {
        match event {
            TrackEvent::Chunk(packet) => self.send_chunk(packet),
            TrackEvent::Discontinuity => self.on_discontinuity(),
        }
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

struct FfmpegInputContext {
    ctx: context::Input,
}

impl FfmpegInputContext {
    fn new(url: &Arc<str>, should_close: Arc<AtomicBool>) -> Result<Self, ffmpeg_next::Error> {
        let ctx = input_with_interrupt_and_dictionary(
            &**url,
            move || should_close.load(Ordering::Relaxed),
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
        )?;
        Ok(Self { ctx })
    }

    fn audio_stream(&self) -> Option<HlsStream> {
        self.stream(Type::Audio)
    }

    fn video_stream(&self) -> Option<HlsStream> {
        self.stream(Type::Video)
    }

    fn stream(&self, kind: Type) -> Option<HlsStream> {
        let stream = self.ctx.streams().best(kind)?;
        Some(HlsStream {
            index: stream.index(),
            time_base: stream.time_base(),
            extradata: stream.read_extradata(),
        })
    }

    /// The HLS demuxer only knows the duration of a playlist that ended
    /// (`#EXT-X-ENDLIST`), so a playlist without one is a live one.
    fn is_live(&self) -> bool {
        self.ctx.duration() <= 0
    }

    fn read_packet(&mut self) -> Result<Packet, ffmpeg_next::Error> {
        let mut packet = Packet::empty();
        packet.read(&mut self.ctx)?;
        Ok(packet)
    }
}

#[derive(Clone)]
struct HlsInputTrackStatsSender {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
    track: TrackKind,
}

impl HlsInputTrackStatsSender {
    fn new(input: &HlsInputContext, track: TrackKind) -> Self {
        Self {
            input_ref: input.input_ref.clone(),
            stats_sender: input.ctx.stats_sender.clone(),
            track,
        }
    }

    fn send_on_packet_received(&self, packet: &Packet) {
        self.send_all([
            HlsInputTrackStatsEvent::PacketReceived,
            HlsInputTrackStatsEvent::BytesReceived(packet.size()),
        ]);
    }

    fn send_on_chunk_released(&self, input_buffer: Duration, effective_buffer: Option<Duration>) {
        self.send_all(
            [
                Some(HlsInputTrackStatsEvent::InputBufferSize(input_buffer)),
                effective_buffer.map(HlsInputTrackStatsEvent::EffectiveBuffer),
            ]
            .into_iter()
            .flatten(),
        );
    }

    fn send(&self, event: HlsInputTrackStatsEvent) {
        self.send_all([event]);
    }

    fn send_all(&self, events: impl IntoIterator<Item = HlsInputTrackStatsEvent>) {
        let events = events
            .into_iter()
            .map(|e| match self.track {
                TrackKind::Video => HlsInputStatsEvent::Video(e).into_event(&self.input_ref),
                TrackKind::Audio => HlsInputStatsEvent::Audio(e).into_event(&self.input_ref),
            })
            .collect::<Vec<_>>();
        self.stats_sender.send(events);
    }
}

const NON_LIVE_DEFAULT_BUFFER: Duration = Duration::from_secs(4);

/// Resolves the buffer options into concrete `(min, desired, max)` values.
///
/// The default covers the common segment size with room for jitter and `min`
/// on top. Specifically:
///   - for segments <= 6s we still have at least 4s for jitter
///   - for segments < 12s we should be able to recover by stretching the buffer (depends on the
///     jitter)
///   - for larger segments there might be issues at the start, but it should recover with time
fn resolve_buffer_options(options: LiveInputBufferOptions) -> (Duration, Duration, Duration) {
    // minimal delta between bounds or above zero
    const D: Duration = Duration::from_millis(500);
    const DEFAULT: Duration = Duration::from_secs(12);
    const DEFAULT_MIN: Duration = Duration::from_secs(2);

    // provided values below the floors are raised instead of rejected
    let options = LiveInputBufferOptions {
        min: options.min.map(|min| Duration::max(min, D)),
        desired: options.desired.map(|desired| Duration::max(desired, D * 2)),
        max: options.max.map(|max| Duration::max(max, D * 3)),
    };

    let desired = match options.desired {
        Some(desired) => desired,
        None => match (options.min, options.max) {
            (Some(min), Some(max)) => (min + max) / 2,
            (Some(min), None) => Duration::max(min + D, DEFAULT),
            (None, Some(max)) => max * 2 / 3,
            (None, None) => DEFAULT,
        },
    };

    // If max not provided default to 2x of desired, but if max is provided
    // then desired defaults to max * (2/3). Intentionally asymmetric.
    let max = Duration::max(options.max.unwrap_or(desired * 2), desired + D);
    let min = Duration::clamp(options.min.unwrap_or(DEFAULT_MIN), D, desired - D);
    (min, desired, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    fn resolve(
        min: Option<u64>,
        desired: Option<u64>,
        max: Option<u64>,
    ) -> (Duration, Duration, Duration) {
        resolve_buffer_options(LiveInputBufferOptions {
            desired: desired.map(ms),
            min: min.map(ms),
            max: max.map(ms),
        })
    }

    #[test]
    fn defaults() {
        assert_eq!(resolve(None, None, None), (ms(2000), ms(12000), ms(24000)));
    }

    #[test]
    fn desired_only() {
        assert_eq!(
            resolve(None, Some(6000), None),
            (ms(2000), ms(6000), ms(12000))
        );
        // the smallest desired with a 500ms floor on min
        assert_eq!(
            resolve(None, Some(1000), None),
            (ms(500), ms(1000), ms(2000))
        );
    }

    #[test]
    fn min_only() {
        // a min below the default desired does not drag desired down
        assert_eq!(
            resolve(Some(4000), None, None),
            (ms(4000), ms(12000), ms(24000))
        );
        assert_eq!(
            resolve(Some(500), None, None),
            (ms(500), ms(12000), ms(24000))
        );
        // desired follows a min above the default
        assert_eq!(
            resolve(Some(20000), None, None),
            (ms(20000), ms(20500), ms(41000))
        );
    }

    #[test]
    fn max_only() {
        // desired follows a smaller max, leaving headroom under it
        assert_eq!(
            resolve(None, None, Some(3000)),
            (ms(1500), ms(2000), ms(3000))
        );
        assert_eq!(
            resolve(None, None, Some(12000)),
            (ms(2000), ms(8000), ms(12000))
        );
        // desired scales with max, it is not capped by the default
        assert_eq!(
            resolve(None, None, Some(30000)),
            (ms(2000), ms(20000), ms(30000))
        );
        // a max that is not divisible by 3 gives a non-round desired
        assert_eq!(
            resolve(None, None, Some(20000)),
            (ms(2000), Duration::from_nanos(13_333_333_333), ms(20000))
        );
        // the smallest max keeps the D margin on both sides
        assert_eq!(
            resolve(None, None, Some(1500)),
            (ms(500), ms(1000), ms(1500))
        );
    }

    #[test]
    fn min_does_not_scale_with_desired() {
        // min stays at the 2s default, so `desired` is what the buffer settles
        // on for a stream whose delivery spread fits below it
        assert_eq!(
            resolve(None, Some(10000), None),
            (ms(2000), ms(10000), ms(20000))
        );
        assert_eq!(
            resolve(None, Some(20000), None),
            (ms(2000), ms(20000), ms(40000))
        );
        // ...until desired is small enough that the 500ms margin binds
        assert_eq!(
            resolve(None, Some(2000), None),
            (ms(1500), ms(2000), ms(4000))
        );
    }

    #[test]
    fn min_and_max() {
        assert_eq!(
            resolve(Some(2000), None, Some(4000)),
            (ms(2000), ms(3000), ms(4000))
        );
    }

    #[test]
    fn all_provided() {
        assert_eq!(
            resolve(Some(5000), Some(6000), Some(9000)),
            (ms(5000), ms(6000), ms(9000))
        );
    }

    #[test]
    fn degenerate_bands() {
        // min == desired == max keeps only the 500ms guardrail margins
        assert_eq!(
            resolve(Some(3000), Some(3000), Some(3000)),
            (ms(2500), ms(3000), ms(3500))
        );
        assert_eq!(
            resolve(Some(1200), Some(1200), Some(1200)),
            (ms(700), ms(1200), ms(1700))
        );
        // min == max without desired
        assert_eq!(
            resolve(Some(4500), None, Some(4500)),
            (ms(4000), ms(4500), ms(5000))
        );
        // band narrower than the 500ms margin
        assert_eq!(
            resolve(Some(1000), Some(1200), Some(1400)),
            (ms(700), ms(1200), ms(1700))
        );
    }

    #[test]
    fn values_below_floors_are_raised() {
        // min >= D, desired >= 2D, max >= 3D
        assert_eq!(
            resolve(Some(0), Some(2), Some(4)),
            (ms(500), ms(1000), ms(1500))
        );
        assert_eq!(
            resolve(None, Some(600), None),
            (ms(500), ms(1000), ms(2000))
        );
        assert_eq!(
            resolve(Some(100), Some(4000), None),
            (ms(500), ms(4000), ms(8000))
        );
        assert_eq!(
            resolve(None, None, Some(1000)),
            (ms(500), ms(1000), ms(1500))
        );
    }
}
