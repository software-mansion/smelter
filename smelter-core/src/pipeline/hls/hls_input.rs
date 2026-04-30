use std::{
    ffi::CString,
    ptr, slice,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use ffmpeg_next::{
    Dictionary, Packet, Stream,
    ffi::{
        avformat_alloc_context, avformat_close_input, avformat_find_stream_info,
        avformat_open_input,
    },
    format::context,
    media::Type,
    util::interrupt,
};
use tracing::{Level, debug, info, span, trace, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        input::Input,
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread},
    },
    queue::{QueueInput, QueueSender, QueueTrackOffset, QueueTrackOptions, WeakQueueInput},
};

use crate::prelude::*;

/// If we assume that max reasonable segment size is 10 second, then
/// range for desired buffer should be larger than the segment (18 second)
const MAX_BUFFER_SIZE: Duration = Duration::from_secs(40);
const DESIRED_MIN_BUFFER_SIZE: Duration = Duration::from_secs(6);
const DESIRED_MAX_BUFFER_SIZE: Duration = Duration::from_secs(24);

/// HLS input - reads from an HLS URL via FFmpeg, demuxes H.264/AAC tracks,
/// decodes, and feeds frames/samples into the queue.
///
/// ## Timestamps
///
/// - FFmpeg opens the HLS URL immediately and discovers tracks.
/// - With offset (`opts.offset = Some(offset)`)
///   - PTS of first frame should be zero
///   - Register track with `QueueTrackOffset::FromStart(offset)`
/// - Without offset (`opts.offset = None`)
///   - PTS of first frame should be zero
///   - Register track with `QueueTrackOffset::None`
/// - On discontinuity (timestamp (per track) changed by 10 seconds)
///   - Send EOS to current decoder threads
///   - Create new queue track `QueueTrackOffset::None`
///   - Ignore packets until `packet.key() == true`
///   - Start new decoder threads
/// - For live stream (`input_ctx.duration() <= 0`)
///   - Estimate buffer size by chunks in channel (input -> decoder). Shift offset
///     to keep buffer between 500 and 1500.
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
        let should_close = Arc::new(AtomicBool::new(false));

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Hls,
        });

        let input_ctx = FfmpegInputContext::new(&opts.url, should_close.clone())?;
        let queue_input = QueueInput::new(&ctx, &input_ref, opts.queue_options);

        if input_ctx.video_stream().is_some()
            && opts.video_decoders.h264 == Some(VideoDecoderOptions::VulkanH264)
            && !ctx.graphics_context.has_vulkan_decoder_support()
        {
            return Err(InputInitError::DecoderError(
                DecoderInitError::VulkanContextRequiredForVulkanDecoder,
            ));
        }

        let mut demuxer = HlsDemuxerThread::new(
            input_ref,
            input_ctx,
            queue_input.downgrade(),
            ctx.clone(),
            opts.video_decoders,
        );

        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: demuxer.has_video,
            audio: demuxer.has_audio,
            offset: match opts.offset {
                Some(offset) => QueueTrackOffset::FromStart(offset),
                None => QueueTrackOffset::None,
            },
        });

        if let Some(sender) = audio_sender {
            demuxer.start_audio_decoder(sender)?;
        }
        if let Some(sender) = video_sender {
            demuxer.spawn_video_decoder(sender)?;
        }

        demuxer.spawn();

        Ok((
            Input::Hls(Self { should_close }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

struct HlsDemuxerThread {
    input_ctx: FfmpegInputContext,
    queue_input: WeakQueueInput,
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    video_decoders: HlsInputVideoDecoders,

    audio: Option<Track>,
    video: Option<Track>,
    first_pts: Option<Duration>,
    has_video: bool,
    has_audio: bool,
}

impl HlsDemuxerThread {
    fn new(
        input_ref: Ref<InputId>,
        input_ctx: FfmpegInputContext,
        queue_input: WeakQueueInput,
        ctx: Arc<PipelineCtx>,
        video_decoders: HlsInputVideoDecoders,
    ) -> Self {
        let has_video = input_ctx.video_stream().is_some();
        let has_audio = input_ctx.audio_stream().is_some();

        Self {
            input_ctx,
            queue_input,
            ctx,
            input_ref,
            video_decoders,
            audio: None,
            video: None,
            first_pts: None,
            has_video,
            has_audio,
        }
    }

    fn spawn(mut self) {
        smelter_render::thread::ThreadRegistry::get().spawn(
            format!("HLS thread for input {}", self.input_ref),
            move || {
                let _span = span!(
                    Level::INFO,
                    "HLS thread",
                    input_id = self.input_ref.to_string()
                )
                .entered();
                self.run();
                info!("Playlist finished")
            },
        );
    }

    fn start_audio_decoder(
        &mut self,
        samples_sender: QueueSender<InputAudioSamples>,
    ) -> Result<(), InputInitError> {
        let Some(stream) = self.input_ctx.audio_stream() else {
            return Ok(());
        };
        // not tested it was always null, but audio is in ADTS, so config is not
        // necessary
        let asc = read_extra_data(&stream);
        let stats_sender = HlsInputTrackStatsSender::new(
            &self.input_ref,
            &self.ctx.stats_sender,
            TrackKind::Audio,
        );
        let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
            self.input_ref.clone(),
            AudioDecoderThreadOptions {
                ctx: self.ctx.clone(),
                decoder_options: FdkAacDecoderOptions { asc },
                samples_sender,
                input_buffer_size: MAX_BUFFER_SIZE,
            },
        )?;

        self.audio = Some(Track {
            index: stream.index(),
            handle,
            time_base: stream.time_base(),
            last_pts: None,
            stats_sender,
        });
        Ok(())
    }

    fn spawn_video_decoder(
        &mut self,
        frame_sender: QueueSender<Frame>,
    ) -> Result<(), InputInitError> {
        let Some(stream) = self.input_ctx.video_stream() else {
            return Ok(());
        };
        let stats_sender = HlsInputTrackStatsSender::new(
            &self.input_ref,
            &self.ctx.stats_sender,
            TrackKind::Video,
        );

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
            ctx: self.ctx.clone(),
            transformer: h264_config.map(H264AvccToAnnexB::new),
            frame_sender,
            input_buffer_size: MAX_BUFFER_SIZE,
        };

        let vulkan_supported = self.ctx.graphics_context.has_vulkan_decoder_support();
        let h264_decoder = self.video_decoders.h264.unwrap_or({
            match vulkan_supported {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        });

        let handle = match h264_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                    &self.input_ref,
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
                    &self.input_ref,
                    decoder_thread_options,
                )?
            }
            _ => {
                return Err(InputInitError::InvalidVideoDecoderProvided {
                    expected: VideoCodec::H264,
                });
            }
        };

        self.video = Some(Track {
            index: stream.index(),
            handle,
            time_base: stream.time_base(),
            last_pts: None,
            stats_sender,
        });
        Ok(())
    }

    fn run(&mut self) {
        loop {
            let packet = match self.input_ctx.read_packet() {
                Ok(packet) => packet,
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(err) => {
                    warn!("HLS read error {err:?}");
                    continue;
                }
            };

            let stream_id = packet.stream();
            let pts = packet.pts().unwrap_or(0);
            trace!(
                ?stream_id,
                pts,
                key = packet.is_key(),
                corrupted = packet.is_corrupt()
            );

            match self.handle_packet(packet) {
                Ok(_) => {
                    // to keep buffer in range for live playlists
                    self.maybe_shift_pts(stream_id);
                }
                Err(HandlePacketError::WaitingForKeyframe) => debug!("Waiting for keyframe"),
                Err(HandlePacketError::UnknownStream) => trace!(stream_id, "Unknown stream"),
                Err(HandlePacketError::CorruptedPacket) => warn!("Detected corrupted packet"),
                Err(HandlePacketError::Discontinuity) => {
                    warn!("Detected discontinuity");
                    self.restart_tracks();
                }
            }
        }

        self.send_eos();
    }

    fn maybe_shift_pts(&mut self, stream_id: usize) {
        if !self.input_ctx.is_live() {
            return;
        }

        let Some(first_pts) = &mut self.first_pts else {
            return;
        };

        if let Some(track) = self.audio.as_mut().or(self.video.as_mut())
            && track.index == stream_id
        {
            let buffered = track.handle.chunk_sender.buffered_duration();
            if buffered > DESIRED_MAX_BUFFER_SIZE {
                *first_pts = first_pts.saturating_add(Duration::from_micros(100))
            }
            if buffered < DESIRED_MIN_BUFFER_SIZE {
                *first_pts = first_pts.saturating_sub(Duration::from_micros(100))
            }
        }
    }

    fn handle_packet(&mut self, packet: Packet) -> Result<(), HandlePacketError> {
        if let Some(track) = &mut self.video
            && packet.stream() == track.index
        {
            if packet.is_corrupt() {
                return Err(HandlePacketError::CorruptedPacket);
            }
            return track.handle_video_packet(packet, &mut self.first_pts);
        }

        if let Some(track) = &mut self.audio
            && packet.stream() == track.index
        {
            if packet.is_corrupt() {
                return Err(HandlePacketError::CorruptedPacket);
            }
            return track.handle_audio_packet(packet, &mut self.first_pts);
        }

        Err(HandlePacketError::UnknownStream)
    }

    fn send_eos(&mut self) {
        if let Some(track) = self.audio.take() {
            if track.handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS. Channel closed")
            }
            debug!("audio thread flushed");
        }
        if let Some(track) = self.video.take() {
            if track.handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS. Channel closed")
            }
            debug!("video thread flushed");
        }
    }

    fn restart_tracks(&mut self) {
        // Flush and send EOS to current decoder threads
        debug!("Replacing processing threads");
        self.send_eos();

        let Some(queue_input) = self.queue_input.upgrade() else {
            return;
        };

        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: self.has_video,
            audio: self.has_audio,
            offset: QueueTrackOffset::None,
        });

        if let Some(sender) = audio_sender {
            self.start_audio_decoder(sender).unwrap();
        }
        if let Some(sender) = video_sender {
            self.spawn_video_decoder(sender).unwrap();
        }

        // Reset timestamp state so new track starts from zero
        self.first_pts = None;
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct Track {
    index: usize,
    handle: DecoderThreadHandle,
    time_base: ffmpeg_next::Rational,
    last_pts: Option<Duration>,
    stats_sender: HlsInputTrackStatsSender,
}

enum HandlePacketError {
    Discontinuity,
    WaitingForKeyframe,
    CorruptedPacket,
    UnknownStream,
}

impl Track {
    fn handle_video_packet(
        &mut self,
        packet: Packet,
        first_pts: &mut Option<Duration>,
    ) -> Result<(), HandlePacketError> {
        let (pts, dts) = self.pts_dts_from_packet(&packet, first_pts)?;
        self.stats_sender.send_on_packet_received(&packet, pts);
        let chunk = EncodedInputChunk {
            data: Bytes::copy_from_slice(packet.data().unwrap()),
            pts,
            dts,
            kind: MediaKind::Video(VideoCodec::H264),
            present: true,
        };

        trace!(?chunk, stream_id = self.index, "Sending video chunk");
        let sender = &self.handle.chunk_sender;
        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            debug!("Channel closed");
        }

        Ok(())
    }

    fn handle_audio_packet(
        &mut self,
        packet: Packet,
        first_pts: &mut Option<Duration>,
    ) -> Result<(), HandlePacketError> {
        let (pts, dts) = self.pts_dts_from_packet(&packet, first_pts)?;
        self.stats_sender.send_on_packet_received(&packet, pts);
        let chunk = EncodedInputChunk {
            data: Bytes::copy_from_slice(packet.data().unwrap()),
            pts,
            dts,
            kind: MediaKind::Audio(AudioCodec::Aac),
            present: true,
        };

        trace!(?chunk, stream_id = self.index, "Sending audio chunk");
        let sender = &self.handle.chunk_sender;
        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            debug!("Channel closed");
        }

        Ok(())
    }

    fn pts_dts_from_packet(
        &mut self,
        packet: &Packet,
        first_pts: &mut Option<Duration>,
    ) -> Result<(Duration, Option<Duration>), HandlePacketError> {
        /// (10s) This value was picked arbitrarily but it's quite conservative.
        const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

        if self.last_pts.is_none() && !packet.is_key() {
            return Err(HandlePacketError::WaitingForKeyframe);
        }

        let pts_timestamp = packet.pts().unwrap_or(0);
        let dts_timestamp = packet.dts();

        let pts = self.timestamp_to_duration(pts_timestamp);
        let dts = dts_timestamp.map(|dts| self.timestamp_to_duration(dts));

        if let Some(last_pts) = self.last_pts
            && last_pts.abs_diff(pts) > DISCONTINUITY_THRESHOLD
        {
            self.stats_sender
                .send(HlsInputTrackStatsEvent::DiscontinuityDetected);
            return Err(HandlePacketError::Discontinuity);
        }
        self.last_pts = Some(pts);

        let first_pts = *first_pts.get_or_insert(pts);

        Ok((pts.saturating_sub(first_pts), dts))
    }

    fn timestamp_to_duration(&self, timestamp: i64) -> Duration {
        Duration::from_secs_f64(
            f64::max(timestamp as f64, 0.0) * self.time_base.numerator() as f64
                / self.time_base.denominator() as f64,
        )
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

    fn is_live(&self) -> bool {
        self.ctx.duration() <= 0
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

#[derive(Clone, Copy)]
enum TrackKind {
    Audio,
    Video,
}

#[derive(Clone)]
struct HlsInputTrackStatsSender {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
    track: TrackKind,
}

impl HlsInputTrackStatsSender {
    fn new(input_ref: &Ref<InputId>, stats_sender: &StatsSender, track: TrackKind) -> Self {
        Self {
            input_ref: input_ref.clone(),
            stats_sender: stats_sender.clone(),
            track,
        }
    }

    fn send_on_packet_received(&self, packet: &Packet, packet_pts: Duration) {
        let chunk_size = packet.size();
        let effective_buffer = packet_pts;
        let events = [
            HlsInputTrackStatsEvent::PacketReceived,
            HlsInputTrackStatsEvent::BytesReceived(chunk_size),
            HlsInputTrackStatsEvent::InputBufferSize(Duration::ZERO),
            HlsInputTrackStatsEvent::EffectiveBuffer(effective_buffer),
        ];
        let events = events
            .into_iter()
            .map(|e| match self.track {
                TrackKind::Video => HlsInputStatsEvent::Video(e).into_event(&self.input_ref),
                TrackKind::Audio => HlsInputStatsEvent::Audio(e).into_event(&self.input_ref),
            })
            .collect::<Vec<_>>();
        self.stats_sender.send(events);
    }

    fn send(&self, event: HlsInputTrackStatsEvent) {
        let event = match self.track {
            TrackKind::Video => HlsInputStatsEvent::Video(event),
            TrackKind::Audio => HlsInputStatsEvent::Audio(event),
        };
        self.stats_sender.send(event.into_event(&self.input_ref));
    }
}
