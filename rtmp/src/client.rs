use std::collections::HashMap;

use tracing::{debug, warn};

use crate::{
    AudioChannels, ExCapabilities, RtmpAudioCodec, RtmpConnectionError, RtmpEvent, RtmpVideoCodec,
    TrackKey,
    client::negotiation::{
        NegotiationProgress, send_connect, send_create_stream, send_publish,
        warn_on_unsupported_codecs,
    },
    error::RtmpStreamError,
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
    /// Video codecs advertised to the server during `connect`. Defaults to [H264, VP8, VP9].
    pub video_codecs: Vec<RtmpVideoCodec>,
    /// Audio codecs advertised to the server during `connect`. Defaults to [AAC, Opus].
    pub audio_codecs: Vec<RtmpAudioCodec>,
}

impl RtmpClientConfig {
    /// Build a config with all known codecs advertised.
    pub fn new(host: String, port: u16, app: String, stream_key: String, use_tls: bool) -> Self {
        Self {
            host,
            port,
            app,
            stream_key,
            use_tls,
            video_codecs: vec![
                RtmpVideoCodec::H264,
                RtmpVideoCodec::Vp8,
                RtmpVideoCodec::Vp9,
            ],
            audio_codecs: vec![RtmpAudioCodec::Aac, RtmpAudioCodec::Opus],
        }
    }

    /// Override the video codecs advertised to the server during `connect`.
    pub fn with_video_codecs(mut self, video_codecs: Vec<RtmpVideoCodec>) -> Self {
        self.video_codecs = video_codecs;
        self
    }

    /// Override the audio codecs advertised to the server during `connect`.
    pub fn with_audio_codecs(mut self, audio_codecs: Vec<RtmpAudioCodec>) -> Self {
        self.audio_codecs = audio_codecs;
        self
    }
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
            // Some servers support Enhanced RTMP media packets without advertising
            // capability bits in the connect response (for example MediaMTX via gortmplib).
            // Send enhanced video packets optimistically and let the server reject them if needed.
            RtmpEvent::VideoData(data) => RtmpMessageOutgoing::Video {
                video: VideoMessage::Data(data),
                stream_id: self.stream_id,
            },
            RtmpEvent::VideoConfig(config) => RtmpMessageOutgoing::Video {
                video: VideoMessage::Config(config),
                stream_id: self.stream_id,
            },
            RtmpEvent::AudioData(data) => {
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
                }
            }
            RtmpEvent::AudioConfig(config) => {
                let channels = config.channels;
                self.state
                    .audio_channels
                    .insert(TrackKey::new(self.stream_id, config.track_id), channels);
                RtmpMessageOutgoing::Audio {
                    audio: AudioMessage::Config(config),
                    stream_id: self.stream_id,
                    channels,
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

            if let Some(response) = state.try_match_connect_response(&msg)? {
                let ex_capabilities = ExCapabilities::from_connect_response(
                    &response.properties,
                    &response.information,
                );
                self.stream.set_writer_ex_capabilities(ex_capabilities);
                warn_on_unsupported_codecs(&response, config);
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

            if let Some((_on_status, stream_id)) = state.try_match_on_status(&msg) {
                return Ok(stream_id);
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
