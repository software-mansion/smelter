use compositor_render::OutputId;
use crossbeam_channel::{bounded, Sender};
use packet_stream::RtpBinaryPacketStream;
use rand::Rng;
use rtp_audio_thread::RtpAudioTrackThreadHandle;
use rtp_video_thread::RtpVideoTrackThreadHandle;
use std::sync::{atomic::AtomicBool, Arc};
use tracing::{debug, span, Level};
use webrtc::rtcp;

use crate::pipeline::encoder::vulkan_h264::VulkanH264Encoder;
use crate::pipeline::rtp::rtp_output::rtp_audio_thread::{
    RtpAudioTrackThread, RtpAudioTrackThreadOptions,
};
use crate::pipeline::rtp::rtp_output::rtp_video_thread::{
    RtpVideoTrackThread, RtpVideoTrackThreadOptions,
};
use crate::prelude::*;
use crate::thread_utils::InitializableThread;
use crate::{
    event::Event,
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder,
        },
        output::{Output, OutputAudio, OutputVideo},
        rtp::{
            payloader::{PayloadedCodec, PayloaderOptions, PayloadingError},
            RtpPacket,
        },
    },
};

mod packet_stream;
mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

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

#[derive(Debug)]
pub enum RtpEvent {
    Data(RtpPacket),
    AudioEos(rtcp::goodbye::Goodbye),
    VideoEos(rtcp::goodbye::Goodbye),
    Err(PayloadingError),
}

impl RtpOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: RtpOutputOptions,
    ) -> Result<(Self, Port), OutputInitError> {
        let mtu = options.connection_options.mtu();

        let (socket, port) = match &options.connection_options {
            RtpOutputConnectionOptions::Udp { port, ip } => udp::udp_socket(ip, *port)?,
            RtpOutputConnectionOptions::TcpServer { port } => tcp_server::tcp_socket(*port)?,
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
                    RtpOutputConnectionOptions::Udp { .. } => {
                        udp::run_udp_sender_thread(socket, rtp_stream)
                    }
                    RtpOutputConnectionOptions::TcpServer { .. } => {
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
            VideoEncoderOptions::FfmpegH264(options) => {
                RtpVideoTrackThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    RtpVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        payloader_options: payloader_options(PayloadedCodec::H264, mtu),
                        chunks_sender: sender,
                    },
                )?
            }
            VideoEncoderOptions::VulkanH264(options) => {
                RtpVideoTrackThread::<VulkanH264Encoder>::spawn(
                    output_id.clone(),
                    RtpVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        payloader_options: payloader_options(PayloadedCodec::H264, mtu),
                        chunks_sender: sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp8(options) => {
                RtpVideoTrackThread::<FfmpegVp8Encoder>::spawn(
                    output_id.clone(),
                    RtpVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        payloader_options: payloader_options(PayloadedCodec::Vp8, mtu),
                        chunks_sender: sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp9(options) => {
                RtpVideoTrackThread::<FfmpegVp9Encoder>::spawn(
                    output_id.clone(),
                    RtpVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        payloader_options: payloader_options(PayloadedCodec::Vp9, mtu),
                        chunks_sender: sender,
                    },
                )?
            }
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
            AudioEncoderOptions::Opus(options) => RtpAudioTrackThread::<OpusEncoder>::spawn(
                output_id.clone(),
                RtpAudioTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options.clone(),
                    payloader_options: payloader_options(PayloadedCodec::Opus, 48_000, mtu),
                    chunks_sender: sender,
                },
            )?,
            AudioEncoderOptions::FdkAac(_options) => {
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
        OutputProtocolKind::Rtp
    }
}
