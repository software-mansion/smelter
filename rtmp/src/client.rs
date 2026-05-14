use std::collections::HashMap;

use tracing::{debug, warn};

use crate::{
    AudioChannels, RtmpAudioCodec, RtmpConnectionError, RtmpEvent, RtmpSerializationMode,
    RtmpVideoCodec, TrackKey, VideoCodecConversionError,
    client::negotiation::{
        NegotiatedCapabilities, NegotiationProgress, PublishStatus, send_connect,
        send_create_stream, send_publish,
    },
    error::{RtmpMessageSerializeError, RtmpStreamError},
    message::{
        AudioMessage, CONTROL_MESSAGE_STREAM_ID, CommandMessage, DataMessage, RtmpMessageIncoming,
        RtmpMessageOutgoing, UserControlMessage, VideoMessage,
    },
    protocol::{
        byte_stream::RtmpByteStream, handshake::Handshake, message_stream::RtmpMessageStream,
    },
    transport::RtmpTransport,
    utils::ShutdownCondition,
};

mod negotiation;

pub struct RtmpClientConfig {
    pub host: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    pub use_tls: bool,
    pub serialization_mode: RtmpSerializationMode,
}

impl RtmpClientConfig {
    fn tc_url(&self) -> String {
        let scheme = if self.use_tls { "rtmps" } else { "rtmp" };
        format!("{}://{}:{}/{}", scheme, self.host, self.port, self.app)
    }
}

pub struct RtmpClient {
    state: RtmpClientState,
    stream_id: u32,
    shutdown_condition: ShutdownCondition,
}

struct RtmpClientState {
    stream: RtmpMessageStream,
    peer_supports_enhanced: bool,
    peer_supports_modex: bool,
    serialization_mode: RtmpSerializationMode,
    audio_channels: HashMap<TrackKey, AudioChannels>,
    /// window size for data incoming from the server
    window_size: Option<u64>,
    /// last ack sent to client
    last_ack: u64,
}

impl RtmpClient {
    pub fn connect(config: RtmpClientConfig) -> Result<Self, RtmpConnectionError> {
        let shutdown_condition = ShutdownCondition::default();

        let transport = if config.use_tls {
            RtmpTransport::tls_client(&config.host, config.port)?
        } else {
            RtmpTransport::tcp_client(&config.host, config.port)?
        };
        let mut socket = RtmpByteStream::new(transport, shutdown_condition.clone());

        Handshake::perform_as_client(&mut socket)?;
        debug!("Handshake complete");

        let mut state = RtmpClientState {
            stream: RtmpMessageStream::new(socket),
            peer_supports_enhanced: false,
            peer_supports_modex: false,
            serialization_mode: config.serialization_mode,
            audio_channels: HashMap::new(),
            window_size: None,
            last_ack: 0,
        };

        let stream_id = state.negotiate_connection(&config)?;
        debug!("Negotiation complete");

        Ok(Self {
            state,
            stream_id,
            shutdown_condition,
        })
    }

    pub fn send<T>(&mut self, event: T) -> Result<(), RtmpStreamError>
    where
        RtmpEvent: From<T>,
    {
        let event = match RtmpEvent::from(event) {
            RtmpEvent::VideoData(data) => {
                if self.state.peer_missing_enhanced_video(data.codec) {
                    return Err(RtmpMessageSerializeError::from(
                        VideoCodecConversionError::UnsupportedLegacyRtmp(data.codec),
                    )
                    .into());
                }
                RtmpMessageOutgoing::Video {
                    video: VideoMessage::Data(data),
                    stream_id: self.stream_id,
                    serialization_mode: self.state.active_serialization_mode(),
                }
            }
            RtmpEvent::VideoConfig(config) => {
                if self.state.peer_missing_enhanced_video(config.codec) {
                    return Err(RtmpMessageSerializeError::from(
                        VideoCodecConversionError::UnsupportedLegacyRtmp(config.codec),
                    )
                    .into());
                }
                RtmpMessageOutgoing::Video {
                    video: VideoMessage::Config(config),
                    stream_id: self.stream_id,
                    serialization_mode: self.state.active_serialization_mode(),
                }
            }
            RtmpEvent::AudioData(data) => {
                if self.state.peer_missing_enhanced_audio(data.codec) {
                    return Err(RtmpMessageSerializeError::InternalError(format!(
                        "Enhanced RTMP is required for audio codec {:?}",
                        data.codec
                    ))
                    .into());
                }
                let channels = self
                    .state
                    .audio_channels
                    .get(&TrackKey::new(self.stream_id, data.track_id))
                    .copied()
                    .unwrap_or(AudioChannels::Stereo);
                RtmpMessageOutgoing::Audio {
                    audio: AudioMessage::Data(data),
                    stream_id: self.stream_id,
                    channels,
                    serialization_mode: self.state.active_serialization_mode(),
                }
            }
            RtmpEvent::AudioConfig(config) => {
                if self.state.peer_missing_enhanced_audio(config.codec) {
                    return Err(RtmpMessageSerializeError::InternalError(format!(
                        "Enhanced RTMP is required for audio codec {:?}",
                        config.codec
                    ))
                    .into());
                }
                let channels = config.channels;
                self.state
                    .audio_channels
                    .insert(TrackKey::new(self.stream_id, config.track_id), channels);
                RtmpMessageOutgoing::Audio {
                    audio: AudioMessage::Config(config),
                    stream_id: self.stream_id,
                    channels,
                    serialization_mode: self.state.active_serialization_mode(),
                }
            }
            RtmpEvent::Metadata(metadata) => RtmpMessageOutgoing::DataMessage {
                data: DataMessage::OnMetaData(metadata),
                stream_id: self.stream_id,
            },
        };
        self.state.stream.write_msg(event)?;

        // try read any pending messages
        while let Some(msg) = self.state.stream.try_read_msg()? {
            self.state.default_msg_handler(msg)?;
        }
        Ok(())
    }
}

impl Drop for RtmpClient {
    fn drop(&mut self) {
        let _ = self
            .state
            .stream
            .write_msg(RtmpMessageOutgoing::CommandMessage {
                msg: CommandMessage::DeleteStream {
                    transaction_id: 0,
                    stream_id: self.stream_id,
                },
                stream_id: CONTROL_MESSAGE_STREAM_ID,
            });
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl RtmpClientState {
    fn active_serialization_mode(&self) -> RtmpSerializationMode {
        match self.serialization_mode {
            RtmpSerializationMode::Enhanced if !self.peer_supports_modex => {
                RtmpSerializationMode::EnhancedNoModEx
            }
            mode => mode,
        }
    }

    fn peer_missing_enhanced_video(&self, codec: RtmpVideoCodec) -> bool {
        match self.serialization_mode {
            RtmpSerializationMode::Auto => {
                codec != RtmpVideoCodec::H264 && !self.peer_supports_enhanced
            }
            RtmpSerializationMode::Enhanced | RtmpSerializationMode::EnhancedNoModEx => false,
        }
    }

    fn peer_missing_enhanced_audio(&self, codec: RtmpAudioCodec) -> bool {
        match self.serialization_mode {
            RtmpSerializationMode::Auto => {
                codec != RtmpAudioCodec::Aac && !self.peer_supports_enhanced
            }
            RtmpSerializationMode::Enhanced | RtmpSerializationMode::EnhancedNoModEx => false,
        }
    }

    fn negotiate_connection(
        &mut self,
        config: &RtmpClientConfig,
    ) -> Result<u32, RtmpConnectionError> {
        let mut state = NegotiationProgress::WaitingForConnectResult;
        send_connect(&mut self.stream, config)?;

        loop {
            let msg = match self.stream.read_msg() {
                Ok(msg) => msg,
                Err(RtmpStreamError::ParseMessage(err)) => {
                    warn!(%err, "Received unknown msg");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };

            if let Some((_response, capabilities)) = state.try_match_connect_response(&msg)? {
                let NegotiatedCapabilities {
                    supports_enhanced,
                    supports_modex,
                } = capabilities;
                self.peer_supports_enhanced = supports_enhanced;
                self.peer_supports_modex = supports_modex;
                state = NegotiationProgress::WaitingForCreateStreamResult;
                send_create_stream(&mut self.stream)?;
                continue;
            }

            if let Some(response) = state.try_match_create_stream_response(&msg)? {
                state = NegotiationProgress::WaitingForOnStatus {
                    stream_id: response.stream_id,
                };
                send_publish(&mut self.stream, &config.stream_key, response.stream_id)?;

                // should be after StreamBegin but e.g. YouTube does not send it
                self.stream
                    .write_msg(RtmpMessageOutgoing::SetChunkSize { chunk_size: 4096 })?;
                self.stream.set_writer_chunk_size(4096);
                continue;
            }

            if let Some(status) = state.try_match_on_status(&msg) {
                match status {
                    PublishStatus::Started { stream_id } => return Ok(stream_id),
                    PublishStatus::Rejected(reason) => {
                        return Err(RtmpConnectionError::ErrorOnPublish(reason));
                    }
                }
            }

            self.default_msg_handler(msg)?
        }
    }

    fn default_msg_handler(&mut self, msg: RtmpMessageIncoming) -> Result<(), RtmpStreamError> {
        match msg {
            RtmpMessageIncoming::SetChunkSize { chunk_size } => {
                self.stream.set_reader_chunk_size(chunk_size as usize);
            }
            RtmpMessageIncoming::WindowAckSize { window_size } => {
                // Client does not receive much data, so sending ACKs
                // will be very rare.
                self.window_size = Some(window_size as u64);
            }
            RtmpMessageIncoming::Acknowledgement { .. } => {
                // TODO: throttle sending based on acks
            }
            RtmpMessageIncoming::SetPeerBandwidth { bandwidth, .. } => {
                // It configures how often client will be sending ACKs,
                // it is different that self.window_size
                self.stream.write_msg(RtmpMessageOutgoing::WindowAckSize {
                    window_size: bandwidth,
                })?;
            }
            RtmpMessageIncoming::UserControl(UserControlMessage::PingRequest { timestamp }) => {
                let msg = UserControlMessage::PingResponse { timestamp };
                self.stream.write_msg(msg.into())?;
            }
            _ => {
                debug!(?msg, "Unhandled message")
            }
        }

        // not sure if it is necessary for client
        self.maybe_send_ack()?;

        Ok(())
    }

    fn maybe_send_ack(&mut self) -> Result<(), RtmpStreamError> {
        let Some(window_size) = self.window_size else {
            return Ok(());
        };
        let bytes_received = self.stream.bytes_read();
        if bytes_received.saturating_sub(self.last_ack) > window_size / 2 {
            self.stream
                .write_msg(RtmpMessageOutgoing::Acknowledgement {
                    bytes_received: (bytes_received % (u32::MAX as u64 + 1)) as u32,
                })?;
            self.last_ack = bytes_received;
        }
        Ok(())
    }
}
