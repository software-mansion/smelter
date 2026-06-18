use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use bytes::{BufMut, Bytes, BytesMut};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError, bounded, select};
use libsrt::SrtSocket;
use mpegts::{DEFAULT_AUDIO_PID, DEFAULT_VIDEO_PID, Muxer, MuxerConfig, MuxerInput, TS_CLOCK_HZ};
use smelter_render::OutputId;
use tracing::{Level, debug, info, span, warn};

use crate::{
    event::Event,
    pipeline::{
        encoder::{
            encoder_thread_audio::{
                AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
            },
            encoder_thread_video::{
                VideoEncoderThread, VideoEncoderThreadHandle, VideoEncoderThreadOptions,
            },
            fdk_aac::FdkAacEncoder,
            ffmpeg_h264::FfmpegH264Encoder,
            vulkan_h264::VulkanH264Encoder,
        },
        output::{Output, OutputAudio, OutputVideo},
        srt::server::SrtOutputsState,
    },
    utils::InitializableThread,
};

use crate::prelude::*;

/// Number of TS packets per SRT live-mode datagram (7 * 188 = 1316 bytes).
const SRT_LIVE_PAYLOAD_PACKETS: usize = 7;
const SRT_LIVE_PAYLOAD_SIZE: usize = SRT_LIVE_PAYLOAD_PACKETS * mpegts::TS_PACKET_SIZE;

/// How long the sender thread waits between shutdown checks while idle.
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(500);

pub struct SrtOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
    shutdown: Arc<AtomicBool>,
    outputs_state: SrtOutputsState,
    output_ref: Ref<OutputId>,
}

impl SrtOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: SrtOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let SrtOutputOptions {
            video: video_opts,
            audio: audio_opts,
            encryption,
        } = options;

        let Some(srt_state) = ctx.srt_state.clone() else {
            return Err(SrtServerError::ServerNotRunning.into());
        };

        let stream_id: Arc<str> = output_ref.id().0.clone();

        let socket_rx = srt_state.outputs.add_output(
            &output_ref,
            stream_id.clone(),
            encryption,
            &srt_state.inputs,
        )?;

        let audio_sample_rate = audio_opts.as_ref().map(|a| a.sample_rate());
        let audio_channels = audio_opts.as_ref().map(|a| a.channels());

        let (video_handle, video_rx) = match video_opts {
            Some(opts) => {
                let (sender, receiver) = bounded(1000);
                let handle = init_video_encoder(&ctx, &output_ref, opts, sender)?;
                (Some(handle), Some(receiver))
            }
            None => (None, None),
        };

        let (audio_handle, audio_rx) = match audio_opts {
            Some(opts) => {
                let (sender, receiver) = bounded(1000);
                let handle = init_audio_encoder(&ctx, &output_ref, opts, sender)?;
                (Some(handle), Some(receiver))
            }
            None => (None, None),
        };

        let shutdown = Arc::new(AtomicBool::new(false));
        let keyframe_request_sender = video_handle
            .as_ref()
            .map(|v| v.keyframe_request_sender.clone());

        let sender = SrtSenderThread {
            socket_rx,
            shutdown: shutdown.clone(),
            video_rx,
            audio_rx,
            keyframe_request_sender,
            audio_sample_rate,
            audio_channels,
        };
        let thread_output_ref = output_ref.clone();
        let thread_stream_id = stream_id.clone();
        thread::Builder::new()
            .name(format!("SRT sender for output {output_ref}"))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "SRT sender",
                    output_id = thread_output_ref.to_string(),
                    stream_id = thread_stream_id.as_ref(),
                )
                .entered();
                sender.run();
                ctx.event_emitter
                    .emit(Event::OutputDone(thread_output_ref.id().clone()));
                debug!("Closing SRT sender thread.");
            })
            .unwrap();

        Ok(Self {
            video: video_handle,
            audio: audio_handle,
            shutdown,
            outputs_state: srt_state.outputs.clone(),
            output_ref,
        })
    }
}

impl Drop for SrtOutput {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.outputs_state.remove_output(&self.output_ref);
    }
}

impl Output for SrtOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Srt
    }
}

fn init_video_encoder(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    options: VideoEncoderOptions,
    chunks_sender: Sender<EncodedOutputEvent>,
) -> Result<VideoEncoderThreadHandle, OutputInitError> {
    let handle = match options {
        VideoEncoderOptions::FfmpegH264(options) => VideoEncoderThread::<FfmpegH264Encoder>::spawn(
            output_ref.clone(),
            VideoEncoderThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                chunks_sender,
            },
        )?,
        VideoEncoderOptions::VulkanH264(options) => {
            if !ctx.graphics_context.has_vulkan_encoder_support() {
                return Err(OutputInitError::EncoderError(
                    EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                ));
            }
            VideoEncoderThread::<VulkanH264Encoder>::spawn(
                output_ref.clone(),
                VideoEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    chunks_sender,
                },
            )?
        }
        VideoEncoderOptions::FfmpegVp8(_) | VideoEncoderOptions::FfmpegVp9(_) => {
            return Err(SrtOutputError::UnsupportedVideoCodec.into());
        }
    };
    Ok(handle)
}

fn init_audio_encoder(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    options: AudioEncoderOptions,
    chunks_sender: Sender<EncodedOutputEvent>,
) -> Result<AudioEncoderThreadHandle, OutputInitError> {
    let handle = match options {
        AudioEncoderOptions::FdkAac(options) => AudioEncoderThread::<FdkAacEncoder>::spawn(
            output_ref.clone(),
            AudioEncoderThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                chunks_sender,
            },
        )?,
        AudioEncoderOptions::Opus(_) => {
            return Err(SrtOutputError::UnsupportedAudioCodec.into());
        }
    };
    Ok(handle)
}

struct SrtSenderThread {
    socket_rx: Receiver<SrtSocket>,
    shutdown: Arc<AtomicBool>,
    video_rx: Option<Receiver<EncodedOutputEvent>>,
    audio_rx: Option<Receiver<EncodedOutputEvent>>,
    keyframe_request_sender: Option<Sender<()>>,
    audio_sample_rate: Option<u32>,
    audio_channels: Option<AudioChannels>,
}

enum WaitResult {
    Caller(SrtSocket),
    Shutdown,
    EncoderClosed,
}

impl SrtSenderThread {
    fn run(self) {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }

            let caller = match self.wait_for_caller() {
                WaitResult::Caller(sock) => sock,
                WaitResult::Shutdown => return,
                WaitResult::EncoderClosed => return,
            };

            // The accepted socket inherits the listener's non-blocking flag.
            // Flip it back to blocking — we rely on blocking sends to provide
            // natural back-pressure and to surface peer disconnects as send
            // errors.
            if let Err(err) = caller.set_nonblocking(false) {
                warn!("Failed to set SRT caller socket to blocking: {err}");
                continue;
            }
            info!("SRT caller connected");

            // Request a keyframe so the new caller can start decoding as soon
            // as possible.
            if let Some(sender) = &self.keyframe_request_sender {
                let _ = sender.send(());
            }

            self.run_session(caller);
            info!("SRT caller disconnected");
        }
    }

    fn wait_for_caller(&self) -> WaitResult {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return WaitResult::Shutdown;
            }
            // Drop encoder chunks that arrive while no caller is connected.
            if drain_encoder(&self.video_rx) || drain_encoder(&self.audio_rx) {
                return WaitResult::EncoderClosed;
            }
            match self.socket_rx.recv_timeout(IDLE_POLL_TIMEOUT) {
                Ok(sock) => return WaitResult::Caller(sock),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return WaitResult::Shutdown,
            }
        }
    }

    fn run_session(&self, caller: SrtSocket) {
        let mut muxer = Muxer::new(MuxerConfig::h264_aac());
        let mut write_buf = BytesMut::with_capacity(SRT_LIVE_PAYLOAD_SIZE * 2);

        let mut pending_video: Option<EncodedOutputChunk> = None;
        let mut pending_audio: Option<EncodedOutputChunk> = None;
        let mut video_eos = self.video_rx.is_none();
        let mut audio_eos = self.audio_rx.is_none();

        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }

            let need_video = !video_eos && pending_video.is_none();
            let need_audio = !audio_eos && pending_audio.is_none();

            match (need_video, need_audio) {
                (true, true) => {
                    let video_rx = self.video_rx.as_ref().unwrap();
                    let audio_rx = self.audio_rx.as_ref().unwrap();
                    select! {
                        recv(video_rx) -> msg => match msg {
                            Ok(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                            _ => video_eos = true,
                        },
                        recv(audio_rx) -> msg => match msg {
                            Ok(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                            _ => audio_eos = true,
                        },
                    }
                }
                (true, false) => match self.video_rx.as_ref().unwrap().recv() {
                    Ok(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                    _ => video_eos = true,
                },
                (false, true) => match self.audio_rx.as_ref().unwrap().recv() {
                    Ok(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                    _ => audio_eos = true,
                },
                (false, false) => {
                    let muxed = match (&pending_video, &pending_audio) {
                        (Some(v), Some(a)) => {
                            if v.pts <= a.pts {
                                mux_video(&mut muxer, pending_video.take().unwrap())
                            } else {
                                mux_audio(
                                    &mut muxer,
                                    pending_audio.take().unwrap(),
                                    self.audio_sample_rate.unwrap(),
                                    self.audio_channels.unwrap(),
                                )
                            }
                        }
                        (Some(_), None) => mux_video(&mut muxer, pending_video.take().unwrap()),
                        (None, Some(_)) => mux_audio(
                            &mut muxer,
                            pending_audio.take().unwrap(),
                            self.audio_sample_rate.unwrap(),
                            self.audio_channels.unwrap(),
                        ),
                        (None, None) => return,
                    };

                    write_buf.extend_from_slice(&muxed);
                    if !flush_srt(&caller, &mut write_buf) {
                        return;
                    }
                }
            }
        }
    }
}

/// Try to drain one queued event from the encoder receiver. Returns `true` if
/// the channel has been disconnected (i.e. the encoder is gone and we should
/// stop).
fn drain_encoder(rx: &Option<Receiver<EncodedOutputEvent>>) -> bool {
    let Some(rx) = rx else {
        return false;
    };
    loop {
        match rx.try_recv() {
            Ok(_) => {}
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => return true,
        }
    }
}

fn mux_video(muxer: &mut Muxer, chunk: EncodedOutputChunk) -> Bytes {
    let pts_ticks = duration_to_ticks(chunk.pts);
    let dts_ticks = chunk.dts.map(duration_to_ticks);
    muxer.write(MuxerInput {
        pid: DEFAULT_VIDEO_PID,
        pts: Some(pts_ticks),
        dts: dts_ticks,
        is_keyframe: chunk.is_keyframe,
        data: &chunk.data,
    })
}

fn mux_audio(
    muxer: &mut Muxer,
    chunk: EncodedOutputChunk,
    sample_rate: u32,
    channels: AudioChannels,
) -> Bytes {
    let pts_ticks = duration_to_ticks(chunk.pts);
    let adts = wrap_in_adts(&chunk.data, sample_rate, channels);
    muxer.write(MuxerInput {
        pid: DEFAULT_AUDIO_PID,
        pts: Some(pts_ticks),
        dts: None,
        is_keyframe: false,
        data: &adts,
    })
}

/// Send as many full SRT live-mode payloads as possible from the buffer.
/// Returns `false` if the peer is gone and the session should end.
fn flush_srt(caller: &SrtSocket, buf: &mut BytesMut) -> bool {
    while buf.len() >= SRT_LIVE_PAYLOAD_SIZE {
        let chunk = buf.split_to(SRT_LIVE_PAYLOAD_SIZE);
        if caller.send(&chunk).is_err() {
            return false;
        }
    }
    true
}

fn duration_to_ticks(duration: Duration) -> u64 {
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos() as u64;
    secs.saturating_mul(TS_CLOCK_HZ) + (nanos * TS_CLOCK_HZ) / 1_000_000_000
}

/// Prepend a 7-byte ADTS header to a raw AAC access unit.
///
/// FDK-AAC is configured with `TRANSMUX=0` (raw bitstream + AudioSpecificConfig
/// as extradata), which is what RTMP/MP4 need. MPEG-TS with the `AacAdts`
/// stream type expects each PES payload to be self-describing, so we synthesize
/// the ADTS header here from the encoder's known parameters.
fn wrap_in_adts(raw: &[u8], sample_rate: u32, channels: AudioChannels) -> Bytes {
    const ADTS_HEADER_LEN: usize = 7;
    // AAC-LC (AOT=2); ADTS profile is AOT-1 = 1.
    const AAC_LC_PROFILE: u8 = 1;

    let freq_index = sample_rate_index(sample_rate);
    let channel_cfg = match channels {
        AudioChannels::Mono => 1u8,
        AudioChannels::Stereo => 2u8,
    };
    let frame_len = (ADTS_HEADER_LEN + raw.len()) as u32;

    let mut out = BytesMut::with_capacity(ADTS_HEADER_LEN + raw.len());
    // Syncword 0xFFF, MPEG-4 ID=0, layer=00, protection_absent=1.
    out.put_u8(0xFF);
    out.put_u8(0xF1);
    // profile (2b) | freq_index (4b) | private (1b) | channel_cfg high bit (1b)
    out.put_u8((AAC_LC_PROFILE << 6) | (freq_index << 2) | ((channel_cfg >> 2) & 0x01));
    // channel_cfg low 2 bits (2b) | original/copy (1b) | home (1b)
    // | copyright id bit (1b) | copyright id start (1b)
    // | frame_length high 2 bits (bits 12-11)
    out.put_u8(((channel_cfg & 0x03) << 6) | ((frame_len >> 11) & 0x03) as u8);
    // frame_length middle 8 bits (bits 10-3)
    out.put_u8(((frame_len >> 3) & 0xFF) as u8);
    // frame_length low 3 bits (bits 2-0) | buffer_fullness high 5 bits
    // Use 0x7FF (VBR) for buffer_fullness.
    out.put_u8((((frame_len & 0x07) as u8) << 5) | 0x1F);
    // buffer_fullness low 6 bits | number_of_raw_data_blocks (2b, = 0).
    out.put_u8(0xFC);

    out.extend_from_slice(raw);
    out.freeze()
}

fn sample_rate_index(sample_rate: u32) -> u8 {
    match sample_rate {
        96000 => 0,
        88200 => 1,
        64000 => 2,
        48000 => 3,
        44100 => 4,
        32000 => 5,
        24000 => 6,
        22050 => 7,
        16000 => 8,
        12000 => 9,
        11025 => 10,
        8000 => 11,
        7350 => 12,
        _ => 15,
    }
}
