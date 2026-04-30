use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use libsrt::{EPOLL_ERR, EPOLL_IN, EpollEvent, SrtEpoll, SrtSocket};
use mpegts::{Demuxer, DemuxerEvent, EsPacket, StreamType, TS_CLOCK_HZ, TS_PACKET_SIZE};
use smelter_render::InputId;
use tracing::{Level, debug, info, span, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        srt::srt_input::state::SrtInputState,
        utils::H264AvccToAnnexB,
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions},
    utils::InitializableThread,
};

use crate::prelude::*;

const SRT_RECV_BUFFER_SIZE: usize = TS_PACKET_SIZE * 7;
const SRT_POLL_TIMEOUT_MS: i64 = 500;
const CHUNK_BUFFER_DURATION: Duration = Duration::from_secs(2);
const SRT_BUFFER: Duration = Duration::from_secs(2);

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &mut SrtInputState,
    sock: SrtSocket,
) -> Option<JoinHandle<()>> {
    let queue_input = input.queue_input.upgrade()?;

    let has_video = input.video.is_some();
    let has_audio = input.audio.is_some();

    let offset = if input.first_connection {
        match input.offset {
            Some(offset) => QueueTrackOffset::FromStart(offset),
            None => QueueTrackOffset::None,
        }
    } else {
        QueueTrackOffset::Pts(ctx.queue_ctx.effective_last_pts() + SRT_BUFFER)
    };

    let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
        video: has_video,
        audio: has_audio,
        offset,
    });

    input.first_connection = false;

    let video_handle = match (&input.video, video_sender) {
        (Some(video), Some(sender)) => {
            match spawn_video_decoder(&ctx, input_ref, video.decoder, sender) {
                Ok(h) => Some(h),
                Err(err) => {
                    warn!(?err, "Failed to spawn SRT video decoder");
                    None
                }
            }
        }
        _ => None,
    };
    let audio_handle = match (&input.audio, audio_sender) {
        (Some(_), Some(sender)) => match spawn_audio_decoder(&ctx, input_ref, sender) {
            Ok(h) => Some(h),
            Err(err) => {
                warn!(?err, "Failed to spawn SRT audio decoder");
                None
            }
        },
        _ => None,
    };

    let input_id = input_ref.to_string();
    let input_ref_owned = input_ref.clone();
    let handle = thread::Builder::new()
        .name(format!("SRT thread for input {input_id}"))
        .spawn(move || {
            let _span = span!(Level::INFO, "SRT thread", input_id = input_id).entered();
            info!(?input_ref_owned, "SRT peer connected");
            run_demuxer_loop(sock, video_handle, audio_handle);
            info!(?input_ref_owned, "SRT input stream ended");
        })
        .unwrap();
    Some(handle)
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoder: VideoDecoderOptions,
    frame_sender: QueueSender<Frame>,
) -> Result<DecoderThreadHandle, InputInitError> {
    let options = VideoDecoderThreadOptions::<H264AvccToAnnexB> {
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

fn run_demuxer_loop(
    sock: SrtSocket,
    video_handle: Option<DecoderThreadHandle>,
    audio_handle: Option<DecoderThreadHandle>,
) {
    let epoll = match SrtEpoll::new() {
        Ok(e) => e,
        Err(err) => {
            warn!("Failed to create SRT epoll for connection: {err}");
            send_eos(&video_handle, &audio_handle);
            return;
        }
    };
    if let Err(err) = epoll.add(&sock, EPOLL_IN | EPOLL_ERR) {
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
        match read_from_srt(&sock, &epoll, &mut buf) {
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

    send_eos(&video_handle, &audio_handle);
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

fn read_from_srt<'a>(socket: &SrtSocket, epoll: &SrtEpoll, buf: &'a mut [u8]) -> ReadOutcome<'a> {
    let mut events = [EpollEvent { sock: 0, events: 0 }; 1];
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

fn ticks_to_duration(ticks: u64) -> Duration {
    let secs = ticks / TS_CLOCK_HZ;
    let rem = ticks % TS_CLOCK_HZ;
    let nanos = (rem * 1_000_000_000) / TS_CLOCK_HZ;
    Duration::new(secs, nanos as u32)
}
