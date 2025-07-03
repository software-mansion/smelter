use compositor_render::OutputId;
use crossbeam_channel::{bounded, Sender};
use packet_stream::RtpBinaryPacketStream;
use payloader::{PayloadedCodec, PayloaderOptions, PayloadingError};
use rand::Rng;
use rtp_audio_thread::{spawn_rtp_audio_thread, RtpAudioTrackThreadHandle};
use rtp_video_thread::{spawn_rtp_video_thread, RtpVideoTrackThreadHandle};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use tracing::{debug, span, Level};

use crate::{
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, opus::OpusEncoder, AudioEncoderOptions,
            VideoEncoderOptions,
        },
        rtp::RequestedPort,
        AudioCodec, PipelineCtx, Port,
    },
};

use super::{Output, OutputAudio, OutputKind, OutputVideo};

mod packet_stream;
mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

pub(super) mod payloader;

pub(crate) struct RtpOutput {
    /// should_close will be set after output is unregistered,
    /// but the primary way of controlling the shutdown is a channel
    /// receiver.
    ///
    /// RtpSender should be explicitly closed based on this value
    /// only if TCP connection is disconnected or writes hang for a
    /// long time.
    should_close: Arc<AtomicBool>,

    audio: Option<RtpAudioTrackThreadHandle>,
    video: Option<RtpVideoTrackThreadHandle>,
}

#[derive(Debug, Clone)]
pub struct RtpSenderOptions {
    pub connection_options: RtpConnectionOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug)]
pub enum RtpEvent {
    Data(RtpPacket),
    AudioEos(rtcp::goodbye::Goodbye),
    VideoEos(rtcp::goodbye::Goodbye),
    Err(PayloadingError),
}

#[derive(Debug)]
pub struct RtpPacket {
    pub packet: rtp::packet::Packet,
    pub timestamp: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpConnectionOptions {
    Udp { port: Port, ip: Arc<str> },
    TcpServer { port: RequestedPort },
}

impl RtpConnectionOptions {
    fn mtu(&self) -> usize {
        match self {
            RtpConnectionOptions::Udp { .. } => 1400,
            RtpConnectionOptions::TcpServer { .. } => 64000,
        }
    }
}

impl RtpOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: RtpSenderOptions,
    ) -> Result<(Self, Port), OutputInitError> {
        let mtu = options.connection_options.mtu();

        let (socket, port) = match &options.connection_options {
            RtpConnectionOptions::Udp { port, ip } => udp::udp_socket(ip, *port)?,
            RtpConnectionOptions::TcpServer { port } => tcp_server::tcp_socket(*port)?,
        };

        let (rtp_sender, rtp_receiver) = bounded(1);

        let video = match options.video {
            Some(video) => Some(Self::init_video_thread(
                &ctx,
                &output_id,
                mtu,
                video,
                rtp_sender.clone(),
            )?),
            None => None,
        };
        let audio = match options.audio {
            Some(audio) => Some(Self::init_audio_thread(
                &ctx,
                &output_id,
                mtu,
                audio,
                rtp_sender.clone(),
            )?),
            None => None,
        };

        let rtp_stream = RtpBinaryPacketStream {
            receiver: rtp_receiver,
            waiting_audio_eos: audio.is_some(),
            waiting_video_eos: video.is_some(),
        };

        let should_close = Arc::new(AtomicBool::new(false));
        let connection_options = options.connection_options;
        let should_close2 = should_close.clone();
        std::thread::Builder::new()
            .name(format!("RTP sender for output {output_id}"))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "RTP sender", output_id = output_id.to_string()).entered();
                match connection_options {
                    RtpConnectionOptions::Udp { .. } => {
                        udp::run_udp_sender_thread(socket, rtp_stream)
                    }
                    RtpConnectionOptions::TcpServer { .. } => {
                        tcp_server::run_tcp_sender_thread(socket, should_close2, rtp_stream)
                    }
                }
                ctx.event_emitter.emit(Event::OutputDone(output_id));
                debug!("Closing RTP sender thread.")
            })
            .unwrap();

        Ok((
            Self {
                should_close,
                audio,
                video,
            },
            port,
        ))
    }

    fn init_video_thread(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        mtu: usize,
        options: VideoEncoderOptions,
        sender: Sender<RtpEvent>,
    ) -> Result<RtpVideoTrackThreadHandle, OutputInitError> {
        fn payloader_options(codec: PayloadedCodec, mtu: usize) -> PayloaderOptions {
            PayloaderOptions {
                codec,
                payload_type: 96,
                clock_rate: 90000,
                mtu,
                ssrc: rand::thread_rng().gen::<u32>(),
            }
        }

        let thread_handle = match &options {
            VideoEncoderOptions::H264(options) => spawn_rtp_video_thread::<FfmpegH264Encoder>(
                ctx.clone(),
                output_id.clone(),
                options.clone(),
                payloader_options(PayloadedCodec::H264, mtu),
                sender,
            )?,
            VideoEncoderOptions::VP8(options) => spawn_rtp_video_thread::<FfmpegVp8Encoder>(
                ctx.clone(),
                output_id.clone(),
                options.clone(),
                payloader_options(PayloadedCodec::Vp8, mtu),
                sender,
            )?,
            VideoEncoderOptions::VP9(options) => spawn_rtp_video_thread::<FfmpegVp9Encoder>(
                ctx.clone(),
                output_id.clone(),
                options.clone(),
                payloader_options(PayloadedCodec::Vp9, mtu),
                sender,
            )?,
        };
        Ok(thread_handle)
    }

    fn init_audio_thread(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        mtu: usize,
        options: AudioEncoderOptions,
        sender: Sender<RtpEvent>,
    ) -> Result<RtpAudioTrackThreadHandle, OutputInitError> {
        fn payloader_options(
            codec: PayloadedCodec,
            sample_rate: u32,
            mtu: usize,
        ) -> PayloaderOptions {
            PayloaderOptions {
                codec,
                payload_type: 97,
                clock_rate: sample_rate,
                mtu,
                ssrc: rand::thread_rng().gen::<u32>(),
            }
        }

        let thread_handle = match options {
            AudioEncoderOptions::Opus(options) => spawn_rtp_audio_thread::<OpusEncoder>(
                ctx.clone(),
                output_id.clone(),
                options.clone(),
                payloader_options(PayloadedCodec::Opus, 48_000, mtu),
                sender,
            )?,
            AudioEncoderOptions::Aac(_options) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Aac))
            }
        };
        Ok(thread_handle)
    }
}

impl Drop for RtpOutput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Output for RtpOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputKind {
        OutputKind::Rtp
    }
}
