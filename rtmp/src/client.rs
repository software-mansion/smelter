use std::{collections::HashMap, thread::JoinHandle};

use tracing::{debug, info, warn};
use url::Url;

use crate::{
    AudioChannels, AudioConfig, RtmpConnectionError, RtmpEvent, TrackKey, VideoConfig,
    amf0::AmfValue,
    client::negotiation::{NegotiationProgress, send_connect, send_create_stream, send_publish},
    error::{RtmpStreamError, TcUrlError},
    ex_capabilities::ExCapabilities,
    message::{
        AudioMessage, CONTROL_MESSAGE_STREAM_ID, CommandMessage, DataMessage, RtmpMessageIncoming,
        RtmpMessageOutgoing, UserControlMessage, VideoMessage,
    },
    protocol::{
        byte_stream::RtmpByteStream, handshake::Handshake, message_stream::RtmpMessageStream,
    },
    reconnect::{ReconnectRequest, resolve_reconnect_url, try_match_reconnect_request},
    transport::RtmpTransport,
    utils::ShutdownCondition,
};

mod negotiation;

#[derive(Clone)]
pub struct RtmpClientConfig {
    pub host: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    pub use_tls: bool,
}

impl RtmpClientConfig {
    pub(crate) fn tc_url(&self) -> String {
        let scheme = if self.use_tls { "rtmps" } else { "rtmp" };
        format!("{}://{}:{}/{}", scheme, self.host, self.port, self.app)
    }

    pub(crate) fn update_with_tc_url(&self, tc_url: &str) -> Result<Self, TcUrlError> {
        let url = Url::parse(tc_url)?;
        let use_tls = match url.scheme() {
            "rtmp" => false,
            "rtmps" => true,
            scheme => return Err(TcUrlError::UnsupportedScheme(scheme.to_string())),
        };
        let host = url.host_str().ok_or(TcUrlError::MissingHost)?.to_string();
        let port = url.port().unwrap_or(if use_tls { 443 } else { 1935 });
        let app = url
            .path()
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            host,
            port,
            app,
            stream_key: self.stream_key.clone(),
            use_tls,
        })
    }
}

pub struct RtmpClient {
    config: RtmpClientConfig,
    state: RtmpClientState,
    stream_id: u32,
    shutdown_condition: ShutdownCondition,
    media_config: MediaConfig,
    pending_reconnect: Option<PendingReconnection>,
}

struct RtmpClientState {
    stream: RtmpMessageStream,
    audio_channels: HashMap<TrackKey, AudioChannels>,
    /// window size for data incoming from the server
    window_size: Option<u64>,
    /// last ack sent to client
    last_ack: u64,
    ex_capabilities: ExCapabilities,
}

#[derive(Default, Clone)]
struct MediaConfig {
    video: Option<VideoConfig>,
    audio: Option<AudioConfig>,
    metadata: Option<HashMap<String, AmfValue>>,
}

struct PendingReconnection {
    handle: JoinHandle<Result<(RtmpClientState, u32), RtmpConnectionError>>,
    new_config: RtmpClientConfig,
    shutdown: ShutdownCondition,
}

impl RtmpClient {
    pub fn connect(config: RtmpClientConfig) -> Result<Self, RtmpConnectionError> {
        let shutdown_condition = ShutdownCondition::default();
        let (state, stream_id) = Self::establish_connection(&config, &shutdown_condition)?;
        Ok(Self {
            config,
            state,
            stream_id,
            shutdown_condition,
            media_config: MediaConfig::default(),
            pending_reconnect: None,
        })
    }

    fn establish_connection(
        config: &RtmpClientConfig,
        shutdown_condition: &ShutdownCondition,
    ) -> Result<(RtmpClientState, u32), RtmpConnectionError> {
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
            ex_capabilities: ExCapabilities::default(),
        };

        let stream_id = state.negotiate_connection(config)?;
        debug!("Negotiation complete");

        Ok((state, stream_id))
    }

    /// Send an RTMP event (audio/video data or config).
    ///
    /// When the server has negotiated E-RTMP `capsEx.Reconnect`, this method
    /// transparently handles `NetConnection.Connect.ReconnectRequest`: the
    /// new connection is established in a background thread while data
    /// continues flowing on the old connection. At the next media boundary
    /// (video keyframe, or any audio chunk for audio-only streams), the
    /// client switches to the new connection and replays codec configs
    /// and metadata.
    pub fn send<T>(&mut self, event: T) -> Result<(), RtmpStreamError>
    where
        RtmpEvent: From<T>,
    {
        let reconnect_ready = self
            .pending_reconnect
            .as_ref()
            .is_some_and(|p| p.handle.is_finished());

        let event = match RtmpEvent::from(event) {
            RtmpEvent::VideoData(data) => {
                if reconnect_ready && data.is_keyframe {
                    self.do_reconnect();
                }
                RtmpMessageOutgoing::Video {
                    video: VideoMessage::Data(data),
                    stream_id: self.stream_id,
                }
            }
            RtmpEvent::VideoConfig(config) => {
                self.media_config.video = Some(config.clone());
                RtmpMessageOutgoing::Video {
                    video: VideoMessage::Config(config),
                    stream_id: self.stream_id,
                }
            }
            RtmpEvent::AudioData(data) => {
                if reconnect_ready && self.media_config.video.is_none() {
                    self.do_reconnect();
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
                }
            }
            RtmpEvent::AudioConfig(config) => {
                self.media_config.audio = Some(config.clone());
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
            RtmpEvent::Metadata(metadata) => {
                self.media_config.metadata = Some(metadata.clone());
                RtmpMessageOutgoing::DataMessage {
                    data: DataMessage::OnMetaData(metadata),
                    stream_id: self.stream_id,
                }
            }
        };
        self.state.stream.write_msg(event)?;

        while let Some(msg) = self.state.stream.try_read_msg()? {
            if let Some(request) = try_match_reconnect_request(&msg) {
                debug!(?request, "Received NetConnection.Connect.ReconnectRequest");
                self.start_reconnect(request);
                continue;
            }
            self.state.default_msg_handler(msg)?;
        }
        Ok(())
    }

    fn do_reconnect(&mut self) {
        let pending = self.pending_reconnect.take().unwrap();

        info!(
            tc_url = %pending.new_config.tc_url(),
            "Completing E-RTMP reconnect"
        );

        let (new_state, stream_id) = match pending.handle.join() {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => {
                warn!(%err, "E-RTMP reconnect failed, continuing on current connection");
                return;
            }
            Err(_) => {
                warn!("E-RTMP reconnect thread panicked, continuing on current connection");
                return;
            }
        };

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

        self.config = pending.new_config;
        self.state = new_state;
        self.stream_id = stream_id;

        if let Some(metadata) = self.media_config.metadata.clone()
            && let Err(err) = self.send(metadata)
        {
            warn!(%err, "Failed to send metadata after reconnect");
        }
        if let Some(config) = self.media_config.video.clone()
            && let Err(err) = self.send(config)
        {
            warn!(%err, "Failed to send video config after reconnect");
        }
        if let Some(config) = self.media_config.audio.clone()
            && let Err(err) = self.send(config)
        {
            warn!(%err, "Failed to send audio config after reconnect");
        }
    }

    fn start_reconnect(&mut self, request: ReconnectRequest) {
        if !self.state.ex_capabilities.reconnect {
            debug!("Ignoring ReconnectRequest: capsEx.Reconnect not negotiated");
            return;
        }

        let current_tc_url = self.config.tc_url();
        let resolved_url = match resolve_reconnect_url(&current_tc_url, request.tc_url.as_deref()) {
            Ok(url) => url,
            Err(err) => {
                warn!(?err, "Ignoring E-RTMP ReconnectRequest with invalid tcUrl");
                return;
            }
        };

        let new_config = match self.config.update_with_tc_url(&resolved_url) {
            Ok(c) => c,
            Err(err) => {
                warn!(
                    ?err,
                    "Ignoring E-RTMP ReconnectRequest with unsupported tcUrl"
                );
                return;
            }
        };

        if let Some(old) = self.pending_reconnect.take() {
            debug!("Canceling previous E-RTMP reconnect in favor of new request");
            old.shutdown.mark_for_shutdown();
        }

        let child_shutdown = self.shutdown_condition.child_condition();
        let thread_config = new_config.clone();
        let thread_shutdown = child_shutdown.clone();
        let handle = std::thread::spawn(move || {
            RtmpClient::establish_connection(&thread_config, &thread_shutdown)
        });

        self.pending_reconnect = Some(PendingReconnection {
            handle,
            new_config,
            shutdown: child_shutdown,
        });
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

            if let Some((_response, ex_capabilities)) = state.try_match_connect_response(&msg)? {
                self.stream.set_writer_ex_capabilities(ex_capabilities);
                self.ex_capabilities = ex_capabilities;
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

            if let Some(request) = try_match_reconnect_request(&msg) {
                debug!(?request, "Received ReconnectRequest during negotiation");
                return Err(RtmpConnectionError::ReconnectRequestedDuringNegotiation(
                    request,
                ));
            }

            self.default_msg_handler(msg)?;
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
