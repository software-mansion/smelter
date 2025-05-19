use std::sync::{atomic::AtomicBool, Arc};

use crate::{
    pipeline::{
        decoder::{self},
        encoder,
        rtp::{BindToPortError, RequestedPort, TransportProtocol},
        types::{EncodedChunk, EncodedChunkKind},
    },
    queue::PipelineEvent,
};
use compositor_render::InputId;
use crossbeam_channel::{bounded, Receiver, Sender};
use rtcp::header::PacketType;
use tracing::{debug, error, span, warn, Level};
use webrtc_util::Unmarshal;

use self::{
    depayloader::{Depayloader, DepayloaderNewError},
    tcp_server::start_tcp_server_thread,
    udp::start_udp_reader_thread,
};

use super::{AudioInputReceiver, Input, InputInitInfo, InputInitResult, VideoInputReceiver};

mod depayloader;
mod tcp_server;
mod udp;

#[derive(Debug, thiserror::Error)]
pub enum RtpReceiverError {
    #[error("Error while setting socket options.")]
    SocketOptions(#[source] std::io::Error),

    #[error("Error while binding the socket.")]
    SocketBind(#[source] std::io::Error),

    #[error("Failed to register input. Port: {0} is already used or not available.")]
    PortAlreadyInUse(u16),

    #[error("Failed to register input. All ports in range {lower_bound} to {upper_bound} are already used or not available.")]
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },

    #[error(transparent)]
    DepayloaderError(#[from] DepayloaderNewError),
}

#[derive(Debug, Clone)]
pub struct RtpReceiverOptions {
    pub port: RequestedPort,
    pub transport_protocol: TransportProtocol,
    pub stream: RtpStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputVideoStream {
    pub options: decoder::VideoDecoderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputAudioStream {
    pub options: decoder::AudioDecoderOptions,
}

#[derive(Debug, Clone)]
pub struct RtpStream {
    pub video: Option<InputVideoStream>,
    pub audio: Option<InputAudioStream>,
}

struct DepayloaderThreadReceivers {
    video: Option<Receiver<PipelineEvent<EncodedChunk>>>,
    audio: Option<Receiver<PipelineEvent<EncodedChunk>>>,
}

pub struct RtpReceiver {
    should_close: Arc<AtomicBool>,
    pub port: u16,
}

impl RtpReceiver {
    pub(super) fn start_new_input(
        input_id: &InputId,
        opts: RtpReceiverOptions,
    ) -> Result<InputInitResult, RtpReceiverError> {
        let should_close = Arc::new(AtomicBool::new(false));

        let (port, packets_rx) = match opts.transport_protocol {
            TransportProtocol::Udp => {
                start_udp_reader_thread(input_id, &opts, should_close.clone())?
            }
            TransportProtocol::TcpServer => {
                start_tcp_server_thread(input_id, &opts, should_close.clone())?
            }
        };

        let depayloader = Depayloader::new(&opts.stream)?;

        let depayloader_receivers =
            Self::start_depayloader_thread(input_id, packets_rx, depayloader);

        let video = match (depayloader_receivers.video, opts.stream.video) {
            (Some(chunk_receiver), Some(stream)) => Some(VideoInputReceiver::Encoded {
                chunk_receiver,
                decoder_options: stream.options,
            }),
            _ => None,
        };
        let audio = match (depayloader_receivers.audio, opts.stream.audio) {
            (Some(chunk_receiver), Some(stream)) => Some(AudioInputReceiver::Encoded {
                chunk_receiver,
                decoder_options: stream.options,
            }),
            _ => None,
        };

        Ok(InputInitResult {
            input: Input::Rtp(Self {
                should_close,
                port: port.0,
            }),
            video,
            audio,
            init_info: InputInitInfo::Rtp { port: Some(port) },
        })
    }

    fn start_depayloader_thread(
        input_id: &InputId,
        receiver: Receiver<bytes::Bytes>,
        depayloader: Depayloader,
    ) -> DepayloaderThreadReceivers {
        let (video_sender, video_receiver) = depayloader
            .video
            .as_ref()
            .map(|_| bounded(5))
            .map_or((None, None), |(tx, rx)| (Some(tx), Some(rx)));
        let (audio_sender, audio_receiver) = depayloader
            .audio
            .as_ref()
            .map(|_| bounded(5))
            .map_or((None, None), |(tx, rx)| (Some(tx), Some(rx)));

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
                run_depayloader_thread(receiver, depayloader, video_sender, audio_sender)
            })
            .unwrap();

        DepayloaderThreadReceivers {
            video: video_receiver,
            audio: audio_receiver,
        }
    }
}

impl Drop for RtpReceiver {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn run_depayloader_thread(
    receiver: Receiver<bytes::Bytes>,
    mut depayloader: Depayloader,
    video_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
    audio_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
) {
    let mut audio_eos_received = audio_sender.as_ref().map(|_| false);
    let mut video_eos_received = video_sender.as_ref().map(|_| false);
    let mut audio_ssrc = None;
    let mut video_ssrc = None;

    let mut maybe_send_video_eos = || {
        if let (Some(sender), Some(false)) = (&video_sender, video_eos_received) {
            video_eos_received = Some(true);
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS from RTP video depayloader. Channel closed.");
            }
        }
    };
    let mut maybe_send_audio_eos = || {
        if let (Some(sender), Some(false)) = (&audio_sender, audio_eos_received) {
            audio_eos_received = Some(true);
            if sender.send(PipelineEvent::EOS).is_err() {
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
                if packet.header.payload_type == 96 && video_ssrc.is_none() {
                    video_ssrc = Some(packet.header.ssrc);
                }
                if packet.header.payload_type == 97 && audio_ssrc.is_none() {
                    audio_ssrc = Some(packet.header.ssrc);
                }

                match depayloader.depayload(packet) {
                    Ok(chunks) => {
                        for chunk in chunks {
                            match &chunk.kind {
                                EncodedChunkKind::Video(_) => {
                                    video_sender.as_ref().map(|video_sender| {
                                        video_sender.send(PipelineEvent::Data(chunk))
                                    })
                                }
                                EncodedChunkKind::Audio(_) => {
                                    audio_sender.as_ref().map(|audio_sender| {
                                        audio_sender.send(PipelineEvent::Data(chunk))
                                    })
                                }
                            };
                        }
                    }
                    Err(err) => {
                        warn!("RTP depayloading error: {}", err);
                        continue;
                    }
                }
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
    #[error("AAC depayoading error")]
    Aac(#[from] depayloader::AacDepayloadingError),
}

impl From<BindToPortError> for RtpReceiverError {
    fn from(value: BindToPortError) -> Self {
        match value {
            BindToPortError::SocketBind(err) => RtpReceiverError::SocketBind(err),
            BindToPortError::PortAlreadyInUse(port) => RtpReceiverError::PortAlreadyInUse(port),
            BindToPortError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            } => RtpReceiverError::AllPortsAlreadyInUse {
                lower_bound,
                upper_bound,
            },
        }
    }
}
