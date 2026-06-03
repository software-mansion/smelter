use std::{collections::HashMap, thread::JoinHandle};

use tracing::{debug, info, warn};
use url::Url;

use crate::{
    AudioChannels, AudioConfig, RtmpConnectionError, RtmpEvent, VideoConfig,
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
    reconnect::{ReconnectRequest, try_match_reconnect_request},
    transport::RtmpTransport,
    utils::ShutdownCondition,
};

mod negotiation;

const OUTGOING_CHUNK_SIZE: u32 = 4096;

#[derive(Clone)]
pub struct RtmpConnectionOptions {
    pub host: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    pub use_tls: bool,
}

impl RtmpConnectionOptions {
    pub(crate) fn tc_url(&self) -> String {
        let scheme = if self.use_tls { "rtmps" } else { "rtmp" };
        format!("{}://{}:{}/{}", scheme, self.host, self.port, self.app)
    }

    /// Resolve an E-RTMP `ReconnectRequest` target into a new config.
    ///
    /// `request_tc_url` is resolved against the current config's tcUrl per the
    /// E-RTMP v2 spec:
    /// - `None` → reconnect to current.
    /// - absolute → used as-is.
    /// - relative / protocol-relative / path-only → resolved against current.
    pub(crate) fn resolve_reconnect(
        &self,
        request_tc_url: Option<&str>,
    ) -> Result<Self, TcUrlError> {
        let current = Url::parse(&self.tc_url())?;
        let url = match request_tc_url {
            Some(request) => current.join(request)?,
            None => current,
        };

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
    connection_opts: RtmpConnectionOptions,
    connection: RtmpConnection,
    shutdown_condition: ShutdownCondition,
    media_config: MediaConfig,
    pending_reconnect: Option<PendingReconnection>,
}

/// A single live RTMP connection: the wire plus all state scoped to it.
/// Everything here dies with the socket and is rebuilt on an E-RTMP reconnect —
/// in contrast to [`RtmpClient`], which owns the durable publishing session.
struct RtmpConnection {
    stream: RtmpMessageStream,
    /// publishing stream id from createStream; valid after negotiation
    stream_id: u32,
    /// window size for data incoming from the server
    window_size: Option<u64>,
    /// last ack sent to client
    last_ack: u64,
    ex_capabilities: ExCapabilities,
}

/// Codec configs and metadata for the stream.
///
/// Single-track only, the wire layer rejects multitrack media messages.
/// Multitrack would make `video`/`audio` per-`TrackId` (`HashMap<TrackId, _>`).
#[derive(Default, Clone)]
struct MediaConfig {
    video: Option<VideoConfig>,
    audio: Option<AudioConfig>,
    metadata: Option<HashMap<String, AmfValue>>,
}

impl MediaConfig {
    /// Record codec configs / metadata so they can be replayed on a new
    /// connection after an E-RTMP reconnect switch.
    fn record(&mut self, event: &RtmpEvent) {
        match event {
            RtmpEvent::VideoConfig(config) => self.video = Some(config.clone()),
            RtmpEvent::AudioConfig(config) => self.audio = Some(config.clone()),
            RtmpEvent::Metadata(metadata) => self.metadata = Some(metadata.clone()),
            RtmpEvent::VideoData(_) | RtmpEvent::AudioData(_) => {}
        }
    }

    fn is_audio_only(&self) -> bool {
        self.video.is_none()
    }

    fn resolve_audio_channels(&self) -> AudioChannels {
        self.audio
            .as_ref()
            .map(|config| config.channels)
            .unwrap_or(AudioChannels::Stereo)
    }

    /// Events to resend on a fresh connection, in dependency order: metadata
    /// first, then codec configs (which must precede the media data that
    /// resumes once the switch completes).
    fn events_to_replay(&self) -> Vec<RtmpEvent> {
        let mut events = Vec::new();
        if let Some(metadata) = self.metadata.clone() {
            events.push(RtmpEvent::Metadata(metadata));
        }
        events.extend(self.video.clone().map(RtmpEvent::VideoConfig));
        events.extend(self.audio.clone().map(RtmpEvent::AudioConfig));
        events
    }
}

struct PendingReconnection {
    handle: JoinHandle<Result<RtmpConnection, RtmpConnectionError>>,
    new_connection_opts: RtmpConnectionOptions,
    shutdown: ShutdownCondition,
}

impl RtmpClient {
    pub fn connect(connection_opts: RtmpConnectionOptions) -> Result<Self, RtmpConnectionError> {
        let shutdown_condition = ShutdownCondition::default();
        let connection =
            RtmpConnection::establish_connection(&connection_opts, &shutdown_condition)?;
        Ok(Self {
            connection_opts,
            connection,
            shutdown_condition,
            media_config: MediaConfig::default(),
            pending_reconnect: None,
        })
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
        let event = RtmpEvent::from(event);

        // Honor a finished background reconnect, but only at a media boundary.
        self.maybe_switch_connection(&event);
        // Remember codec configs / metadata so they can be replayed after a switch.
        self.media_config.record(&event);
        // Encode + write on whatever connection is now current.
        let msg = self.outgoing_message(event);
        self.connection.stream.write_msg(msg)?;

        while let Some(msg) = self.connection.stream.try_read_msg()? {
            self.handle_incoming(msg)?;
        }
        Ok(())
    }

    fn outgoing_message(&self, event: RtmpEvent) -> RtmpMessageOutgoing {
        let stream_id = self.connection.stream_id;
        match event {
            RtmpEvent::VideoData(data) => RtmpMessageOutgoing::Video {
                video: VideoMessage::Data(data),
                stream_id,
            },
            RtmpEvent::VideoConfig(config) => RtmpMessageOutgoing::Video {
                video: VideoMessage::Config(config),
                stream_id,
            },
            RtmpEvent::AudioData(data) => RtmpMessageOutgoing::Audio {
                channels: self.media_config.resolve_audio_channels(),
                audio: AudioMessage::Data(data),
                stream_id,
            },
            RtmpEvent::AudioConfig(config) => {
                let channels = config.channels;
                RtmpMessageOutgoing::Audio {
                    audio: AudioMessage::Config(config),
                    stream_id,
                    channels,
                }
            }
            RtmpEvent::Metadata(metadata) => RtmpMessageOutgoing::DataMessage {
                data: DataMessage::OnMetaData(metadata),
                stream_id,
            },
        }
    }

    fn maybe_switch_connection(&mut self, event: &RtmpEvent) {
        let ready = self
            .pending_reconnect
            .as_ref()
            .is_some_and(|p| p.handle.is_finished());

        // Spec: switch only at a media boundary — a video keyframe, or (for
        // audio-only streams) any audio chunk. (E-RTMPv2, Reconnect message flow.)
        let is_switch_boundary = match event {
            RtmpEvent::VideoData(data) => data.is_keyframe,
            RtmpEvent::AudioData(_) => self.media_config.is_audio_only(),
            _ => false,
        };

        if !ready || !is_switch_boundary {
            return;
        }

        let pending = self.pending_reconnect.take().unwrap();
        info!(
            tc_url = %pending.new_connection_opts.tc_url(),
            "Completing E-RTMP reconnect"
        );

        let new_connection = match pending.handle.join() {
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

        // Tear down the old stream before swapping to the new connection.
        if let Err(err) = self.connection.send_delete_stream() {
            debug!(%err, "Failed to send DeleteStream on old connection during reconnect");
        }

        self.connection_opts = pending.new_connection_opts;
        self.connection = new_connection;

        // The new connection's NetStream has no history, so codec configs and
        // metadata must be resent: a sequence header (AVC/AAC config) MUST
        // precede media on a stream, and onMetaData describes it. The E-RTMP
        // Reconnect flow doesn't spell this out — it follows from NetStream
        // sequence-header semantics.
        for event in self.media_config.events_to_replay() {
            if let Err(err) = self.send(event) {
                warn!(%err, "Failed to replay recorded config after reconnect");
            }
        }
    }

    fn handle_incoming(&mut self, msg: RtmpMessageIncoming) -> Result<(), RtmpStreamError> {
        if let Some(request) = try_match_reconnect_request(&msg) {
            debug!(?request, "Received NetConnection.Connect.ReconnectRequest");
            self.start_reconnect(request);
            return Ok(());
        }
        self.connection.default_msg_handler(msg)
    }

    fn start_reconnect(&mut self, request: ReconnectRequest) {
        if !self.connection.ex_capabilities.reconnect {
            debug!("Ignoring ReconnectRequest: capsEx.Reconnect not negotiated");
            return;
        }

        let new_connection_opts = match self
            .connection_opts
            .resolve_reconnect(request.tc_url.as_deref())
        {
            Ok(c) => c,
            Err(err) => {
                warn!(?err, "Ignoring E-RTMP ReconnectRequest with invalid tcUrl");
                return;
            }
        };

        if let Some(old) = self.pending_reconnect.take() {
            debug!("Canceling previous E-RTMP reconnect in favor of new request");
            old.shutdown.mark_for_shutdown();
        }

        let child_shutdown = self.shutdown_condition.child_condition();
        let thread_connection_opts = new_connection_opts.clone();
        let thread_shutdown = child_shutdown.clone();
        let handle = std::thread::spawn(move || {
            RtmpConnection::establish_connection(&thread_connection_opts, &thread_shutdown)
        });

        self.pending_reconnect = Some(PendingReconnection {
            handle,
            new_connection_opts,
            shutdown: child_shutdown,
        });
    }
}

impl Drop for RtmpClient {
    fn drop(&mut self) {
        if let Err(err) = self.connection.send_delete_stream() {
            debug!(%err, "Failed to send DeleteStream on drop");
        }
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl RtmpConnection {
    fn establish_connection(
        connection_opts: &RtmpConnectionOptions,
        shutdown_condition: &ShutdownCondition,
    ) -> Result<Self, RtmpConnectionError> {
        let transport = if connection_opts.use_tls {
            RtmpTransport::tls_client(&connection_opts.host, connection_opts.port)?
        } else {
            RtmpTransport::tcp_client(&connection_opts.host, connection_opts.port)?
        };
        let mut socket = RtmpByteStream::new(transport, shutdown_condition.clone());

        Handshake::perform_as_client(&mut socket)?;
        debug!("Handshake complete");

        let message_stream = RtmpMessageStream::new(socket);
        let connection = Self::negotiate_connection(message_stream, connection_opts)?;
        debug!("Negotiation complete");

        Ok(connection)
    }

    fn negotiate_connection(
        stream: RtmpMessageStream,
        conn_opts: &RtmpConnectionOptions,
    ) -> Result<Self, RtmpConnectionError> {
        let mut conn = Self {
            stream,
            stream_id: 0,
            window_size: None,
            last_ack: 0,
            ex_capabilities: ExCapabilities::default(),
        };
        let mut state = NegotiationProgress::WaitingForConnectResult;
        send_connect(&mut conn.stream, conn_opts)?;

        loop {
            let msg = match conn.stream.read_msg() {
                Ok(msg) => msg,
                Err(RtmpStreamError::ParseMessage(err)) => {
                    warn!(%err, "Received unknown msg");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };

            if let Some((_response, ex_capabilities)) = state.try_match_connect_response(&msg)? {
                conn.stream.set_writer_ex_capabilities(ex_capabilities);
                conn.ex_capabilities = ex_capabilities;
                state = NegotiationProgress::WaitingForCreateStreamResult;
                send_create_stream(&mut conn.stream)?;
                continue;
            }

            if let Some(response) = state.try_match_create_stream_response(&msg)? {
                state = NegotiationProgress::WaitingForOnStatus {
                    stream_id: response.stream_id,
                };
                send_publish(&mut conn.stream, &conn_opts.stream_key, response.stream_id)?;

                // should be after StreamBegin but e.g. YouTube does not send it
                conn.stream.write_msg(RtmpMessageOutgoing::SetChunkSize {
                    chunk_size: OUTGOING_CHUNK_SIZE,
                })?;
                conn.stream
                    .set_writer_chunk_size(OUTGOING_CHUNK_SIZE as usize);
                continue;
            }

            if let Some((_on_status, stream_id)) = state.try_match_on_status(&msg) {
                conn.stream_id = stream_id;
                return Ok(conn);
            }

            if let Some(request) = try_match_reconnect_request(&msg) {
                debug!(?request, "Received ReconnectRequest during negotiation");
                return Err(RtmpConnectionError::ReconnectRequestedDuringNegotiation(
                    request,
                ));
            }

            conn.default_msg_handler(msg)?;
        }
    }

    fn send_delete_stream(&mut self) -> Result<(), RtmpStreamError> {
        self.stream.write_msg(RtmpMessageOutgoing::CommandMessage {
            msg: CommandMessage::DeleteStream {
                transaction_id: 0,
                stream_id: self.stream_id,
            },
            stream_id: CONTROL_MESSAGE_STREAM_ID,
        })
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
                // it is different from self.window_size
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
