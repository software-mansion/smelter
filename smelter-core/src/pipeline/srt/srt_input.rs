use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use libsrt::{EPOLL_ERR, EPOLL_IN, EpollEvent, SrtEpoll, SrtSocket};
use mpegts::{Demuxer, DemuxerEvent, EsPacket, StreamType, TS_CLOCK_HZ, TS_PACKET_SIZE};
use tracing::{Level, debug, info, span, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        input::Input,
    },
    queue::{QueueInput, QueueSender, QueueTrackOffset, QueueTrackOptions},
    utils::InitializableThread,
};

use crate::prelude::*;

/// Receive buffer used when reading from the SRT socket. Sized to hold several
/// MPEG-TS packets per recv to minimise syscall overhead.
const SRT_RECV_BUFFER_SIZE: usize = TS_PACKET_SIZE * 7;

/// How long the SRT epoll waits before re-checking the shutdown flag.
const SRT_POLL_TIMEOUT_MS: i64 = 500;

/// How long the accept loop waits between shutdown checks.
const ACCEPT_POLL_TIMEOUT_MS: i64 = 500;

/// Channel capacity between demuxer and decoder threads.
const CHUNK_BUFFER_DURATION: Duration = Duration::from_secs(2);

/// SRT input - listens on an UDP port for an incoming SRT stream, demuxes the
/// MPEG-TS container (pure Rust, via the `mpegts` crate) and forwards H.264
/// video and AAC audio to the decoders.
///
/// ## Flow
///
/// - A listener SRT socket is bound to the configured port and put into
///   non-blocking mode. An SRT epoll waits for an incoming connection.
/// - On connection, the accepted socket's bytes are streamed into an
///   [`mpegts::Demuxer`]. The demuxer parses PAT/PMT and emits reassembled
///   PES payloads as [`EsPacket`]s.
/// - The thread routes ES packets to the appropriate decoder based on the
///   configured track options.
/// - When the peer disconnects or `SrtInput` is dropped, EOS is sent to the
///   decoders and the thread exits.
pub struct SrtInput {
    shutdown: Arc<AtomicBool>,
}

impl SrtInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: SrtInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        if opts.video.is_none() && opts.audio.is_none() {
            return Err(SrtInputError::NoVideoOrAudio.into());
        }

        let has_video = opts.video.is_some();
        let has_audio = opts.audio.is_some();

        if let Some(video) = &opts.video
            && video.decoder == VideoDecoderOptions::VulkanH264
            && !ctx.graphics_context.has_vulkan_decoder_support()
        {
            return Err(InputInitError::DecoderError(
                DecoderInitError::VulkanContextRequiredForVulkanDecoder,
            ));
        }

        // Bind the listener up-front so registration surfaces bind errors
        // synchronously, before we return from new_input.
        let listener = bind_listener(opts.port)?;

        let shutdown = Arc::new(AtomicBool::new(false));

        let queue_input = QueueInput::new(&ctx, &input_ref, opts.queue_options);
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: has_video,
            audio: has_audio,
            offset: match opts.offset {
                Some(offset) => QueueTrackOffset::FromStart(offset),
                None => QueueTrackOffset::None,
            },
        });

        let video_handle = match (&opts.video, video_sender) {
            (Some(video), Some(sender)) => Some(spawn_video_decoder(
                &ctx,
                &input_ref,
                video.decoder,
                sender,
            )?),
            _ => None,
        };
        let audio_handle = match (&opts.audio, audio_sender) {
            (Some(_), Some(sender)) => Some(spawn_audio_decoder(&ctx, &input_ref, sender)?),
            _ => None,
        };

        let demuxer = SrtDemuxerThread {
            listener,
            input_ref: input_ref.clone(),
            shutdown: shutdown.clone(),
            video_handle,
            audio_handle,
        };
        let demuxer_input_ref = input_ref.clone();
        thread::Builder::new()
            .name(format!("SRT input {demuxer_input_ref}"))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "SRT input",
                    input_id = demuxer_input_ref.to_string()
                )
                .entered();
                demuxer.run();
            })
            .unwrap();

        Ok((
            Input::Srt(Self { shutdown }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for SrtInput {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

fn bind_listener(port: u16) -> Result<SrtSocket, SrtInputError> {
    let socket = SrtSocket::new()?;
    socket.set_nonblocking(true)?;
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    socket
        .bind(addr)
        .map_err(|e| SrtInputError::Bind(port, e))?;
    socket.listen(1)?;
    Ok(socket)
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoder: VideoDecoderOptions,
    frame_sender: QueueSender<Frame>,
) -> Result<DecoderThreadHandle, InputInitError> {
    let options = VideoDecoderThreadOptions::<H264AnnexBPassthrough> {
        ctx: ctx.clone(),
        transformer: None,
        frame_sender,
        input_buffer_size: CHUNK_BUFFER_DURATION,
    };
    let handle =
        match decoder {
            VideoDecoderOptions::FfmpegH264 => VideoDecoderThread::<
                ffmpeg_h264::FfmpegH264Decoder,
                _,
            >::spawn(input_ref.clone(), options)?,
            VideoDecoderOptions::VulkanH264 => VideoDecoderThread::<
                vulkan_h264::VulkanH264Decoder,
                _,
            >::spawn(input_ref.clone(), options)?,
            _ => {
                return Err(SrtInputError::InvalidVideoDecoder.into());
            }
        };
    Ok(handle)
}

// Dummy transformer type; SRT carries H.264 in Annex-B already, so we never
// actually construct one — `transformer: None` is used. The type parameter on
// `VideoDecoderThreadOptions` still needs concrete inference.
type H264AnnexBPassthrough = crate::pipeline::utils::H264AvccToAnnexB;

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    samples_sender: QueueSender<InputAudioSamples>,
) -> Result<DecoderThreadHandle, InputInitError> {
    let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
        input_ref.clone(),
        AudioDecoderThreadOptions {
            ctx: ctx.clone(),
            decoder_options: FdkAacDecoderOptions { asc: None },
            samples_sender,
            input_buffer_size: CHUNK_BUFFER_DURATION,
        },
    )?;
    Ok(handle)
}

struct SrtDemuxerThread {
    listener: SrtSocket,
    input_ref: Ref<InputId>,
    shutdown: Arc<AtomicBool>,
    video_handle: Option<DecoderThreadHandle>,
    audio_handle: Option<DecoderThreadHandle>,
}

impl SrtDemuxerThread {
    fn run(self) {
        let Self {
            listener,
            input_ref,
            shutdown,
            video_handle,
            audio_handle,
        } = self;

        let connection = match wait_for_connection(&listener, &shutdown) {
            Some(sock) => sock,
            None => {
                send_eos(&video_handle, &audio_handle);
                return;
            }
        };
        info!(?input_ref, "SRT peer connected");

        // We no longer need the listener socket — drop it before consuming
        // the accepted connection to keep one connection at a time.
        drop(listener);

        if let Err(err) = connection.set_nonblocking(true) {
            warn!("Failed to set SRT connection to non-blocking: {err}");
            send_eos(&video_handle, &audio_handle);
            return;
        }

        let epoll = match SrtEpoll::new() {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to create SRT epoll for connection: {err}");
                send_eos(&video_handle, &audio_handle);
                return;
            }
        };
        if let Err(err) = epoll.add(&connection, EPOLL_IN | EPOLL_ERR) {
            warn!("Failed to add SRT connection to epoll: {err}");
            send_eos(&video_handle, &audio_handle);
            return;
        }

        let mut demuxer = Demuxer::new();
        let mut video_pid: Option<u16> = None;
        let mut audio_pid: Option<u16> = None;
        let mut first_pts: Option<Duration> = None;
        let mut buf = [0u8; SRT_RECV_BUFFER_SIZE];

        'outer: loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match read_from_srt(&connection, &epoll, &shutdown, &mut buf) {
                ReadOutcome::Data(bytes) => demuxer.push(bytes),
                ReadOutcome::WouldBlock => {}
                ReadOutcome::Eof => {
                    demuxer.flush();
                    drain_events(
                        &mut demuxer,
                        &mut video_pid,
                        &mut audio_pid,
                        &mut first_pts,
                        &video_handle,
                        &audio_handle,
                    );
                    break 'outer;
                }
            }
            drain_events(
                &mut demuxer,
                &mut video_pid,
                &mut audio_pid,
                &mut first_pts,
                &video_handle,
                &audio_handle,
            );
        }

        info!(?input_ref, "SRT input stream ended");
        send_eos(&video_handle, &audio_handle);
    }
}

fn drain_events(
    demuxer: &mut Demuxer,
    video_pid: &mut Option<u16>,
    audio_pid: &mut Option<u16>,
    first_pts: &mut Option<Duration>,
    video_handle: &Option<DecoderThreadHandle>,
    audio_handle: &Option<DecoderThreadHandle>,
) {
    while let Some(event) = demuxer.pop_event() {
        match event {
            DemuxerEvent::ProgramDiscovered(streams) => {
                *video_pid = streams
                    .iter()
                    .find(|s| matches!(s.stream_type, StreamType::H264))
                    .map(|s| s.pid);
                *audio_pid = streams
                    .iter()
                    .find(|s| matches!(s.stream_type, StreamType::AacAdts | StreamType::AacLatm))
                    .map(|s| s.pid);
            }
            DemuxerEvent::EsPacket(pkt) => forward_es_packet(
                pkt,
                video_pid,
                audio_pid,
                first_pts,
                video_handle,
                audio_handle,
            ),
        }
    }
}

fn forward_es_packet(
    pkt: EsPacket,
    video_pid: &Option<u16>,
    audio_pid: &Option<u16>,
    first_pts: &mut Option<Duration>,
    video_handle: &Option<DecoderThreadHandle>,
    audio_handle: &Option<DecoderThreadHandle>,
) {
    let Some(pts_ticks) = pkt.pts else {
        return;
    };
    let pts = ticks_to_duration(pts_ticks);
    let dts = pkt.dts.map(ticks_to_duration);
    let offset = *first_pts.get_or_insert(pts);
    let pts = pts.saturating_sub(offset);
    let dts = dts.map(|dts| dts.saturating_sub(offset));

    if Some(pkt.pid) == *video_pid
        && let Some(handle) = video_handle
    {
        let chunk = EncodedInputChunk {
            data: pkt.data,
            pts,
            dts,
            kind: MediaKind::Video(VideoCodec::H264),
            present: true,
        };
        if handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .is_err()
        {
            debug!("Video decoder channel closed.");
        }
    } else if Some(pkt.pid) == *audio_pid
        && let Some(handle) = audio_handle
    {
        let chunk = EncodedInputChunk {
            data: pkt.data,
            pts,
            dts,
            kind: MediaKind::Audio(AudioCodec::Aac),
            present: true,
        };
        if handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .is_err()
        {
            debug!("Audio decoder channel closed.");
        }
    }
}

enum ReadOutcome<'a> {
    Data(&'a [u8]),
    WouldBlock,
    Eof,
}

fn read_from_srt<'a>(
    socket: &SrtSocket,
    epoll: &SrtEpoll,
    shutdown: &Arc<AtomicBool>,
    buf: &'a mut [u8],
) -> ReadOutcome<'a> {
    let mut events = [EpollEvent { sock: 0, events: 0 }; 1];
    if shutdown.load(Ordering::Relaxed) {
        return ReadOutcome::Eof;
    }
    match epoll.wait(&mut events, SRT_POLL_TIMEOUT_MS) {
        Ok(0) => ReadOutcome::WouldBlock,
        Ok(_) => {
            if events[0].events & EPOLL_ERR != 0 {
                return ReadOutcome::Eof;
            }
            match socket.recv(buf) {
                Ok(0) => ReadOutcome::Eof,
                Ok(n) => ReadOutcome::Data(&buf[..n]),
                Err(err) => {
                    debug!("SRT recv returned error (treating as EOF): {err}");
                    ReadOutcome::Eof
                }
            }
        }
        Err(err) => {
            debug!("SRT epoll_wait returned error (treating as EOF): {err}");
            ReadOutcome::Eof
        }
    }
}

fn send_eos(video: &Option<DecoderThreadHandle>, audio: &Option<DecoderThreadHandle>) {
    if let Some(handle) = video
        && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
    {
        debug!("Failed to send video EOS. Channel closed.");
    }
    if let Some(handle) = audio
        && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
    {
        debug!("Failed to send audio EOS. Channel closed.");
    }
}

fn wait_for_connection(listener: &SrtSocket, shutdown: &Arc<AtomicBool>) -> Option<SrtSocket> {
    let epoll = SrtEpoll::new().ok()?;
    if epoll.add(listener, EPOLL_IN | EPOLL_ERR).is_err() {
        return None;
    }
    let mut events = [EpollEvent { sock: 0, events: 0 }; 1];
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return None;
        }
        match epoll.wait(&mut events, ACCEPT_POLL_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(_) => match listener.accept() {
                Ok((sock, _addr)) => return Some(sock),
                Err(err) => {
                    warn!("SRT accept failed: {err}");
                    return None;
                }
            },
            Err(err) => {
                warn!("SRT epoll wait on listener failed: {err}");
                return None;
            }
        }
    }
}

fn ticks_to_duration(ticks: u64) -> Duration {
    let secs = ticks / TS_CLOCK_HZ;
    let rem = ticks % TS_CLOCK_HZ;
    // rem < 90_000, so rem * 1_000_000_000 fits in u64 (< 9e13).
    let nanos = (rem * 1_000_000_000) / TS_CLOCK_HZ;
    Duration::new(secs, nanos as u32)
}
