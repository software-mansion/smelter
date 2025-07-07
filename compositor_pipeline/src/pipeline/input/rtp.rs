use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

use compositor_render::{Frame, InputId};
use crossbeam_channel::{bounded, Receiver};
use rtcp::header::PacketType;
use tracing::{debug, error, span, warn, Level};
use webrtc_util::Unmarshal;

use self::{tcp_server::start_tcp_server_thread, udp::start_udp_reader_thread};
use super::{Input, InputInitInfo};
use crate::{
    error::DecoderInitError,
    pipeline::{
        decoder::{
            ffmpeg_h264::FfmpegH264Decoder, ffmpeg_vp8::FfmpegVp8Decoder,
            ffmpeg_vp9::FfmpegVp9Decoder, opus, vulkan_h264::VulkanH264, DecodedDataReceiver,
            VideoDecoderOptions,
        },
        input::rtp::{
            depayloader::{
                AudioSpecificConfig, DepayloadedCodec, DepayloaderInitError, DepayloaderOptions,
            },
            rtp_audio_thread::{spawn_rtp_audio_thread, RtpAudioTrackThreadHandle},
            rtp_video_thread::{spawn_rtp_video_thread, RtpVideoTrackThreadHandle},
        },
        output::rtp::RtpPacket,
        rtp::{BindToPortError, RequestedPort, TransportProtocol},
        types::EncodedChunk,
        PipelineCtx,
    },
    queue::PipelineEvent,
};

mod rtp_audio_thread;
mod rtp_video_thread;
mod tcp_server;
mod udp;

pub(crate) mod depayloader;

/// [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
/// [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpAacDepayloaderMode {
    LowBitrate,
    HighBitrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpAudioOptions {
    Opus {
        decoder_options: opus::Options,
    },
    FdkAac {
        asc: AudioSpecificConfig,
        depayloader_mode: RtpAacDepayloaderMode,
    },
}

#[derive(Debug, Clone)]
pub struct RtpInputOptions {
    pub port: RequestedPort,
    pub transport_protocol: TransportProtocol,
    pub video: Option<VideoDecoderOptions>,
    pub audio: Option<RtpAudioOptions>,
}

struct DepayloaderThreadReceivers {
    video: Option<Receiver<PipelineEvent<EncodedChunk>>>,
    audio: Option<Receiver<PipelineEvent<EncodedChunk>>>,
}

pub struct RtpInput {
    should_close: Arc<AtomicBool>,
    pub port: u16,
}

impl RtpInput {
    pub(super) fn new(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: RtpInputOptions,
    ) -> Result<(Input, InputInitInfo, DecodedDataReceiver), RtpInputError> {
        let should_close = Arc::new(AtomicBool::new(false));

        let (port, raw_packets_receiver) = match opts.transport_protocol {
            TransportProtocol::Udp => {
                start_udp_reader_thread(&input_id, &opts, should_close.clone())?
            }
            TransportProtocol::TcpServer => {
                start_tcp_server_thread(&input_id, &opts, should_close.clone())?
            }
        };

        let (video_handle, video_frames_receiver) =
            Self::start_video_thread(&ctx, &input_id, opts.video)?;

        let (audio_handle, audio_samples_receiver) = match opts.audio {
            Some(options) => {
                let (sender, receiver) = bounded(100);
                let handle = match options {
                    RtpAudioOptions::Opus { decoder_options } => spawn_rtp_audio_thread(
                        ctx.clone(),
                        input_id.clone(),
                        48_000,
                        DepayloadedCodec::Opus,
                        DepayloaderOptions {
                            codec: options.into(),
                            clock_rate: 48_000,
                        },
                        sender,
                    ),
                    RtpAudioOptions::FdkAac {
                        asc,
                        depayloader_mode,
                    } => spawn_rtp_audio_thread(
                        ctx.clone(),
                        input_id.clone(),
                        asc.sample_rate,
                        DepayloadedCodec::Aac(depayloader_mode, asc),
                        DepayloaderOptions {
                            codec: options.into(),
                            clock_rate: asc.sample_rate,
                        },
                        sender,
                    ),
                };
                (Some(handle), Some(receiver))
            }
            None => (None, None),
        };

        let depayloader_receivers = Self::start_rtp_demuxer_thread(
            &input_id,
            raw_packets_receiver,
            audio_handle,
            video_handle,
        );

        Ok((
            Input::Rtp(Self {
                should_close,
                port: port.0,
            }),
            InputInitInfo::Rtp { port: Some(port) },
            DecodedDataReceiver {
                video: video_frames_receiver,
                audio: audio_samples_receiver,
            },
        ))
    }

    fn start_rtp_demuxer_thread(
        input_id: &InputId,
        receiver: Receiver<bytes::Bytes>,
        audio: Option<RtpAudioTrackThreadHandle>,
        video: Option<RtpVideoTrackThreadHandle>,
    ) -> DepayloaderThreadReceivers {
        let input_id = input_id.clone();
        std::thread::Builder::new()
            .name(format!("Depayloading thread for input: {}", input_id.0))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "RTP depayloader",
                    input_id = input_id.to_string()
                )
                .entered();
                run_rtp_demuxer_thread(receiver, audio, video)
            })
            .unwrap();
    }

    fn start_video_thread(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
        decoder_options: Option<VideoDecoderOptions>,
    ) -> Result<
        (
            Option<RtpVideoTrackThreadHandle>,
            Option<Receiver<PipelineEvent<Frame>>>,
        ),
        DecoderInitError,
    > {
        let Some(decoder_options) = decoder_options else {
            return Ok((None, None));
        };

        let (sender, receiver) = bounded(100);
        let handle = match decoder_options {
            VideoDecoderOptions::FfmpegH264 => spawn_rtp_video_thread::<FfmpegH264Decoder>(
                ctx.clone(),
                input_id.clone(),
                DepayloaderOptions::H264,
                sender,
            )?,
            VideoDecoderOptions::FfmpegVp8 => spawn_rtp_video_thread::<FfmpegVp8Decoder>(
                ctx.clone(),
                input_id.clone(),
                DepayloaderOptions::Vp8,
                sender,
            )?,
            VideoDecoderOptions::FfmpegVp9 => spawn_rtp_video_thread::<FfmpegVp9Decoder>(
                ctx.clone(),
                input_id.clone(),
                DepayloaderOptions::Vp9,
                sender,
            )?,
            VideoDecoderOptions::VulkanH264 => spawn_rtp_video_thread::<VulkanH264>(
                ctx.clone(),
                input_id.clone(),
                DepayloaderOptions::H264,
                sender,
            )?,
        };
        Ok((Some(handle), Some(receiver)))
    }
    fn start_audio_thread(
        ctx: &Arc<PipelineCtx>,
        input_id: &InputId,
        options: Option<RtpAudioOptions>,
    ) -> Result<
        (
            Option<RtpAudioTrackThreadHandle>,
            Option<Receiver<PipelineEvent<Frame>>>,
        ),
        DecoderInitError,
    > {
        let Some(options) = options else {
            return Ok((None, None));
        };

        let (sender, receiver) = bounded(100);
        let handle = match options {
            RtpAudioOptions::Opus { decoder_options } => spawn_rtp_audio_thread(
                ctx.clone(),
                input_id.clone(),
                48_000,
                DepayloadedCodec::Opus,
                DepayloaderOptions {
                    codec: options.into(),
                    clock_rate: 48_000,
                },
                sender,
            ),
            RtpAudioOptions::FdkAac {
                asc,
                depayloader_mode,
            } => spawn_rtp_audio_thread(
                ctx.clone(),
                input_id.clone(),
                asc.sample_rate,
                DepayloadedCodec::Aac(depayloader_mode, asc),
                DepayloaderOptions {
                    codec: options.into(),
                    clock_rate: asc.sample_rate,
                },
                sender,
            ),
        };
        (Some(handle), Some(receiver))
    }
}

impl Drop for RtpInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn run_rtp_demuxer_thread(
    receiver: Receiver<bytes::Bytes>,
    video_handle: Option<RtpVideoTrackThreadHandle>,
    audio_handle: Option<RtpAudioTrackThreadHandle>,
) {
    let start = Instant::now();
    let mut audio_eos_received = audio_handle.as_ref().map(|_| false);
    let mut video_eos_received = video_handle.as_ref().map(|_| false);
    let mut audio_ssrc = None;
    let mut video_ssrc = None;

    let mut maybe_send_video_eos = || {
        if let (Some(handle), Some(false)) = (&video_handle, video_eos_received) {
            video_eos_received = Some(true);
            if handle.rtp_packet_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS from RTP video depayloader. Channel closed.");
            }
        }
    };
    let mut maybe_send_audio_eos = || {
        if let (Some(handle), Some(false)) = (&audio_handle, audio_eos_received) {
            audio_eos_received = Some(true);
            if handle.rtp_packet_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS from RTP audio depayloader. Channel closed.");
            }
        }
    };
    loop {
        let Ok(mut buffer) = receiver.recv() else {
            debug!("Closing RTP depayloader thread.");
            break;
        };

        match rtp::packet::Packet::unmarshal(&mut buffer.clone()) {
            // https://datatracker.ietf.org/doc/html/rfc5761#section-4
            //
            // Given these constraints, it is RECOMMENDED to follow the guidelines
            // in the RTP/AVP profile [7] for the choice of RTP payload type values,
            // with the additional restriction that payload type values in the range
            // 64-95 MUST NOT be used.
            Ok(packet) if packet.header.payload_type < 64 || packet.header.payload_type > 95 => {
                if packet.header.payload_type == 96 {
                    let video_ssrc = *video_ssrc.get_or_insert(packet.header.ssrc);
                    if let Some(video) = video_handle {
                        video.rtp_packet_sender.send(PipelineEvent::Data(RtpPacket {
                            packet,
                            timestamp: todo!(),
                        }))
                    }
                } else if packet.header.payload_type == 97 {
                    let audio_ssrc = *audio_ssrc.get_or_insert(packet.header.ssrc);
                    if let Some(audio) = audio_handle {
                        audio.rtp_packet_sender.send(PipelineEvent::Data(RtpPacket {
                            packet,
                            timestamp: todo!(),
                        }))
                    }
                }
                ()
            }
            Ok(_) | Err(_) => {
                match rtcp::packet::unmarshal(&mut buffer) {
                    Ok(rtcp_packets) => {
                        for rtcp_packet in rtcp_packets {
                            if let PacketType::Goodbye = rtcp_packet.header().packet_type {
                                for ssrc in rtcp_packet.destination_ssrc() {
                                    if Some(ssrc) == audio_ssrc {
                                        maybe_send_audio_eos()
                                    }
                                    if Some(ssrc) == video_ssrc {
                                        maybe_send_video_eos()
                                    }
                                }
                            } else {
                                debug!(
                                    packet_type=?rtcp_packet.header().packet_type,
                                    "Received RTCP packet"
                                )
                            }
                        }
                    }
                    Err(err) => {
                        warn!(%err, "Received an unexpected packet, which is not recognized either as RTP or RTCP. Dropping.");
                    }
                }
                continue;
            }
        };
    }
    maybe_send_audio_eos();
    maybe_send_video_eos();
}

#[derive(Debug, thiserror::Error)]
pub enum DepayloadingError {
    #[error("Bad payload type {0}")]
    BadPayloadType(u8),
    #[error(transparent)]
    Rtp(#[from] rtp::Error),
    #[error("AAC depayloading error")]
    Aac(#[from] depayloader::AacDepayloadingError),
}

impl From<BindToPortError> for RtpInputError {
    fn from(value: BindToPortError) -> Self {
        match value {
            BindToPortError::SocketBind(err) => RtpInputError::SocketBind(err),
            BindToPortError::PortAlreadyInUse(port) => RtpInputError::PortAlreadyInUse(port),
            BindToPortError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            } => RtpInputError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            },
        }
    }
}

#[derive(Default)]
pub struct RolloverState {
    sync_point: Instant,
    clock_rate: u32,
    previous_timestamp: Option<u32>,
    rollover_count: usize,
}

impl RolloverState {
    fn new(sync_point: Instant, clock_rate: u32) -> Self {
        Self {
            sync_point,
            clock_rate,
            previous_timestamp: None,
            rollover_count: 0,
        }
    }

    fn timestamp(&mut self, current_timestamp: u32) -> u64 {
        let Some(previous_timestamp) = self.previous_timestamp else {
            self.previous_timestamp = Some(current_timestamp);
            return current_timestamp as u64;
        };

        let timestamp_diff = u32::abs_diff(previous_timestamp, current_timestamp);
        if timestamp_diff >= u32::MAX / 2 {
            if previous_timestamp > current_timestamp {
                self.rollover_count += 1;
            } else {
                // We received a packet from before the rollover, so we need to decrement the count
                self.rollover_count = self.rollover_count.saturating_sub(1);
            }
        }

        self.previous_timestamp = Some(current_timestamp);

        (self.rollover_count as u64) * (u32::MAX as u64 + 1) + current_timestamp as u64
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RtpInputError {
    #[error("Error while setting socket options.")]
    SocketOptions(#[source] std::io::Error),

    #[error("Error while binding the socket.")]
    SocketBind(#[source] std::io::Error),

    #[error("Failed to register input. Port: {0} is already used or not available.")]
    PortAlreadyInUse(u16),

    #[error("Failed to register input. All ports in range {lower_bound} to {upper_bound} are already used or not available.")]
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },

    #[error(transparent)]
    DepayloaderError(#[from] DepayloaderInitError),
}
