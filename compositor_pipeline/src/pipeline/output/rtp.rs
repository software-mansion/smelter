use compositor_render::OutputId;
use crossbeam_channel::{Receiver, Sender};
use std::sync::{atomic::AtomicBool, Arc};
use tracing::{debug, span, Level};

use crate::{
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{
            audio_encoder_thread::AudioEncoderThreadHandle, ffmpeg_vp9::FfmpegVp9Encoder, video_encoder_thread::VideoEncoderThreadHandle, AudioEncoderOptions, VideoEncoderOptions
        },
        rtp::RequestedPort,
        types::EncoderOutputEvent,
        PipelineCtx, Port,
    },
};

use self::{packet_stream::PacketStream, payloader::Payloader};

mod packet_stream;
mod payloader;
mod tcp_server;
mod udp;

pub struct RtpOutput {
    pub connection_options: RtpConnectionOptions,

    /// should_close will be set after output is unregistered,
    /// but the primary way of controlling the shutdown is a channel
    /// receiver.
    ///
    /// RtpSender should be explicitly closed based on this value
    /// only if TCP connection is disconnected or writes hang for a
    /// long time.
    should_close: Arc<AtomicBool>,

    audio: Option<AudioEncoderThreadHandle>,
    video: Option<VideoEncoderThreadHandle>,
}

#[derive(Debug, Clone)]
pub struct RtpSenderOptions {
    pub connection_options: RtpConnectionOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpConnectionOptions {
    Udp { port: Port, ip: Arc<str> },
    TcpServer { port: RequestedPort },
}

impl RtpOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: &OutputId,
        options: RtpSenderOptions,
    ) -> Result<(Self, Port), OutputInitError> {
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1);
        let payloader = Payloader::new(options.video, options.audio);
        let mtu = match options.connection_options {
            RtpConnectionOptions::Udp { .. } => 1400,
            RtpConnectionOptions::TcpServer { .. } => 64000,
        };
        let packet_stream = PacketStream::new(packets_receiver, payloader, mtu);

        let (socket, port) = match &options.connection_options {
            RtpConnectionOptions::Udp { port, ip } => udp::udp_socket(ip, *port)?,
            RtpConnectionOptions::TcpServer { port } => tcp_server::tcp_socket(*port)?,
        };

        let video = match &options.video {
            Some(video) => {
                Self::init_video_track(&ctx, output_id, options, encoded_chunks_sender)
            }
            None => None,
        };

        let should_close = Arc::new(AtomicBool::new(false));
        let connection_options = options.connection_options.clone();
        let output_id = output_id.clone();
        let should_close2 = should_close.clone();
        std::thread::Builder::new()
            .name(format!("RTP sender for output {}", output_id))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "RTP sender", output_id = output_id.to_string()).entered();
                match connection_options {
                    RtpConnectionOptions::Udp { .. } => {
                        udp::run_udp_sender_thread(socket, packet_stream)
                    }
                    RtpConnectionOptions::TcpServer { .. } => {
                        tcp_server::run_tcp_sender_thread(socket, should_close2, packet_stream)
                    }
                }
                ctx.event_emitter.emit(Event::OutputDone(output_id));
                debug!("Closing RTP sender thread.")
            })
            .unwrap();

        Ok((
            Self {
                connection_options: options.connection_options,
                should_close,
            },
            port,
        ))
    }

    fn init_video_track(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        options: VideoEncoderOptions,
        encoded_chunks_sender: Sender<EncoderOutputEvent>,
    ) -> Result<(VideoEncoderThreadHandle, usize), OutputInitError> {
        let resolution = options.resolution();

        let encoder = match &options {
            VideoEncoderOptions::H264(options) => {
                Some(VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    ctx.clone(),
                    output_id.clone(),
                    options.clone(),
                    encoded_chunks_sender,
                )?)
            }
            VideoEncoderOptions::VP8(options) => {
                Some(VideoEncoderThread::<FfmpegVp8Encoder>::spawn(
                    ctx.clone(),
                    output_id.clone(),
                    options.clone(),
                    encoded_chunks_sender,
                )?)
            }
            VideoEncoderOptions::VP9(options) => {
                Some(VideoEncoderThread::<FfmpegVp9Encoder>::spawn(
                    ctx.clone(),
                    output_id.clone(),
                    options.clone(),
                    encoded_chunks_sender,
                )?)
            }
        };
    }
}
impl Drop for RtpOutput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
