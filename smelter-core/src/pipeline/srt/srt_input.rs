use std::{
    ffi::{CString, c_int, c_void},
    mem,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    ptr, slice,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use bytes::Bytes;
use ffmpeg_next::ffi::{
    AV_NOPTS_VALUE, AVERROR_EOF, AVFMT_FLAG_CUSTOM_IO, AVFormatContext, AVIOContext, AVMediaType,
    AVPacket, AVRational, AVStream, av_find_input_format, av_free, av_init_packet, av_malloc,
    av_packet_unref, av_read_frame, avformat_alloc_context, avformat_close_input,
    avformat_find_stream_info, avformat_open_input, avio_alloc_context, avio_context_free,
};
use libsrt::{EPOLL_ERR, EPOLL_IN, EpollEvent, SrtEpoll, SrtSocket};
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

/// Buffer filled by the SRT demuxer for AVFormatContext probing.
const AVIO_BUFFER_SIZE: usize = 1316 * 7;

/// How long AVIO read polls SRT before checking the shutdown flag.
const AVIO_POLL_TIMEOUT_MS: i64 = 500;

/// How long the accept loop waits between shutdown checks.
const ACCEPT_POLL_TIMEOUT_MS: i64 = 500;

/// Channel capacity between demuxer and decoder threads.
const CHUNK_BUFFER_DURATION: Duration = Duration::from_secs(2);

/// SRT input - listens on an UDP port for an incoming SRT stream, demuxes the
/// MPEG-TS container (via FFmpeg) and forwards H.264 video and AAC audio to the
/// decoders.
///
/// ## Flow
///
/// - A listener SRT socket is bound to the configured port and put into
///   non-blocking mode. An SRT epoll waits for an incoming connection.
/// - On connection, the accepted socket becomes the source for a custom
///   `AVIOContext` that feeds `avformat_open_input` with `mpegts` forced.
/// - FFmpeg's demuxer reads packets; the thread routes them to the appropriate
///   decoder based on the configured track options.
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

        let avio_ctx = Box::new(AvioSrtCtx {
            socket: connection,
            shutdown: shutdown.clone(),
            epoll: match SrtEpoll::new() {
                Ok(e) => e,
                Err(err) => {
                    warn!("Failed to create SRT epoll for connection: {err}");
                    send_eos(&video_handle, &audio_handle);
                    return;
                }
            },
        });
        if let Err(err) = avio_ctx.epoll.add(&avio_ctx.socket, EPOLL_IN | EPOLL_ERR) {
            warn!("Failed to add SRT connection to epoll: {err}");
            send_eos(&video_handle, &audio_handle);
            return;
        }

        let mut fmt_ctx = match MpegTsFormatContext::open(avio_ctx) {
            Ok(ctx) => ctx,
            Err(err) => {
                warn!("Failed to open MPEG-TS stream over SRT: {err}");
                send_eos(&video_handle, &audio_handle);
                return;
            }
        };

        let video_stream = fmt_ctx.find_stream(AVMediaType::AVMEDIA_TYPE_VIDEO);
        let audio_stream = fmt_ctx.find_stream(AVMediaType::AVMEDIA_TYPE_AUDIO);

        let mut first_pts: Option<Duration> = None;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match fmt_ctx.read_packet() {
                ReadResult::Packet(pkt) => {
                    let stream_index = pkt.stream_index;
                    let time_base = pkt.time_base;
                    let pts_raw = pkt.pts.unwrap_or(0);
                    let dts_raw = pkt.dts;
                    let pts = timestamp_to_duration(pts_raw, time_base);
                    let dts = dts_raw.map(|dts| timestamp_to_duration(dts, time_base));
                    let offset = *first_pts.get_or_insert(pts);
                    let pts = pts.saturating_sub(offset);
                    let dts = dts.map(|dts| dts.saturating_sub(offset));

                    if Some(stream_index) == video_stream
                        && let Some(handle) = &video_handle
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
                    } else if Some(stream_index) == audio_stream
                        && let Some(handle) = &audio_handle
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
                ReadResult::Eof => break,
                ReadResult::Error(err) => {
                    warn!("SRT MPEG-TS read error: {err}");
                    break;
                }
            }
        }

        info!(?input_ref, "SRT input stream ended");
        send_eos(&video_handle, &audio_handle);
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

fn timestamp_to_duration(ts: i64, time_base: AVRational) -> Duration {
    let secs = f64::max(ts as f64, 0.0) * time_base.num as f64 / time_base.den as f64;
    Duration::from_secs_f64(secs)
}

/// Opaque payload for the AVIO read callback.
struct AvioSrtCtx {
    socket: SrtSocket,
    shutdown: Arc<AtomicBool>,
    epoll: SrtEpoll,
}

extern "C" fn avio_read_cb(opaque: *mut c_void, buf: *mut u8, buf_size: c_int) -> c_int {
    let ctx = unsafe { &mut *(opaque as *mut AvioSrtCtx) };
    let mut events = [EpollEvent { sock: 0, events: 0 }; 1];
    loop {
        if ctx.shutdown.load(Ordering::Relaxed) {
            return AVERROR_EOF;
        }
        match ctx.epoll.wait(&mut events, AVIO_POLL_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(_) => {
                if events[0].events & EPOLL_ERR != 0 {
                    return AVERROR_EOF;
                }
                let dst = unsafe { slice::from_raw_parts_mut(buf, buf_size as usize) };
                match ctx.socket.recv(dst) {
                    Ok(0) => return AVERROR_EOF,
                    Ok(n) => return n as c_int,
                    Err(err) => {
                        debug!("SRT recv returned error (treating as EOF): {err}");
                        return AVERROR_EOF;
                    }
                }
            }
            Err(err) => {
                debug!("SRT epoll_wait returned error (treating as EOF): {err}");
                return AVERROR_EOF;
            }
        }
    }
}

/// RAII-wrapper for a custom-IO FFmpeg format context attached to an SRT
/// socket. Owns the `AVIOContext`, the buffer handed to FFmpeg and the boxed
/// opaque payload.
struct MpegTsFormatContext {
    fmt_ctx: *mut AVFormatContext,
    avio: *mut AVIOContext,
    opaque: *mut AvioSrtCtx,
}

impl MpegTsFormatContext {
    fn open(opaque: Box<AvioSrtCtx>) -> Result<Self, ffmpeg_next::Error> {
        unsafe {
            let fmt_ctx = avformat_alloc_context();
            if fmt_ctx.is_null() {
                return Err(ffmpeg_next::Error::from(AVERROR_EOF));
            }

            let buf = av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
            if buf.is_null() {
                avformat_close_input(&mut (fmt_ctx as *mut _));
                return Err(ffmpeg_next::Error::from(AVERROR_EOF));
            }

            let opaque_ptr = Box::into_raw(opaque);
            let avio = avio_alloc_context(
                buf,
                AVIO_BUFFER_SIZE as c_int,
                0,
                opaque_ptr as *mut c_void,
                Some(avio_read_cb),
                None,
                None,
            );
            if avio.is_null() {
                av_free(buf as *mut c_void);
                drop(Box::from_raw(opaque_ptr));
                avformat_close_input(&mut (fmt_ctx as *mut _));
                return Err(ffmpeg_next::Error::from(AVERROR_EOF));
            }

            (*fmt_ctx).pb = avio;
            (*fmt_ctx).flags |= AVFMT_FLAG_CUSTOM_IO as c_int;

            let fmt_name = CString::new("mpegts").unwrap();
            let input_fmt = av_find_input_format(fmt_name.as_ptr());

            let mut fmt_ctx_mut = fmt_ctx;
            let res =
                avformat_open_input(&mut fmt_ctx_mut, ptr::null(), input_fmt, ptr::null_mut());
            if res < 0 {
                // On failure avformat_open_input frees fmt_ctx but not our avio/buf/opaque.
                let avio_buf = (*avio).buffer;
                avio_context_free(&mut (avio as *mut _));
                av_free(avio_buf as *mut c_void);
                drop(Box::from_raw(opaque_ptr));
                return Err(ffmpeg_next::Error::from(res));
            }

            let res = avformat_find_stream_info(fmt_ctx_mut, ptr::null_mut());
            if res < 0 {
                let avio_buf = (*avio).buffer;
                avformat_close_input(&mut fmt_ctx_mut);
                avio_context_free(&mut (avio as *mut _));
                av_free(avio_buf as *mut c_void);
                drop(Box::from_raw(opaque_ptr));
                return Err(ffmpeg_next::Error::from(res));
            }

            Ok(Self {
                fmt_ctx: fmt_ctx_mut,
                avio,
                opaque: opaque_ptr,
            })
        }
    }

    fn find_stream(&self, media_type: AVMediaType) -> Option<i32> {
        unsafe {
            let nb = (*self.fmt_ctx).nb_streams as isize;
            let streams = (*self.fmt_ctx).streams;
            let mut best: Option<i32> = None;
            for i in 0..nb {
                let stream: *mut AVStream = *streams.offset(i);
                if stream.is_null() {
                    continue;
                }
                let codecpar = (*stream).codecpar;
                if codecpar.is_null() {
                    continue;
                }
                if (*codecpar).codec_type == media_type && best.is_none() {
                    best = Some((*stream).index);
                }
            }
            best
        }
    }

    fn read_packet(&mut self) -> ReadResult {
        unsafe {
            let mut pkt: AVPacket = mem::zeroed();
            av_init_packet(&mut pkt);
            let res = av_read_frame(self.fmt_ctx, &mut pkt);
            if res == AVERROR_EOF {
                return ReadResult::Eof;
            }
            if res < 0 {
                return ReadResult::Error(ffmpeg_next::Error::from(res));
            }
            let stream_index = pkt.stream_index;
            let stream = *(*self.fmt_ctx).streams.offset(stream_index as isize);
            let time_base = (*stream).time_base;
            let pts = if pkt.pts == AV_NOPTS_VALUE {
                None
            } else {
                Some(pkt.pts)
            };
            let dts = if pkt.dts == AV_NOPTS_VALUE {
                None
            } else {
                Some(pkt.dts)
            };
            let data = if pkt.data.is_null() || pkt.size <= 0 {
                Bytes::new()
            } else {
                Bytes::copy_from_slice(slice::from_raw_parts(pkt.data, pkt.size as usize))
            };
            av_packet_unref(&mut pkt);
            ReadResult::Packet(RawPacket {
                stream_index,
                time_base,
                pts,
                dts,
                data,
            })
        }
    }
}

impl Drop for MpegTsFormatContext {
    fn drop(&mut self) {
        unsafe {
            let avio_buf = (*self.avio).buffer;
            let mut fmt = self.fmt_ctx;
            avformat_close_input(&mut fmt);
            let mut avio = self.avio;
            avio_context_free(&mut avio);
            av_free(avio_buf as *mut c_void);
            drop(Box::from_raw(self.opaque));
        }
    }
}

enum ReadResult {
    Packet(RawPacket),
    Eof,
    Error(ffmpeg_next::Error),
}

struct RawPacket {
    stream_index: i32,
    time_base: AVRational,
    pts: Option<i64>,
    dts: Option<i64>,
    data: Bytes,
}
