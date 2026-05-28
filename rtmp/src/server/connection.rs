use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::{RtmpEvent, utils::ShutdownCondition};

pub struct RtmpServerConnection {
    pub(super) app: Arc<str>,
    pub(super) stream_key: Arc<str>,
    pub(super) receiver: Receiver<RtmpEvent>,
    pub(super) command_sender: Sender<ServerCommand>,
    pub(super) shutdown_condition: ShutdownCondition,
}

/// Commands the public API can inject into the connection thread.
pub(super) enum ServerCommand {
    /// Send an E-RTMP `NetConnection.Connect.ReconnectRequest` onStatus to the publisher.
    RequestReconnect {
        tc_url: Option<String>,
        description: Option<String>,
    },
}

impl RtmpServerConnection {
    pub fn app(&self) -> &Arc<str> {
        &self.app
    }

    pub fn stream_key(&self) -> &Arc<str> {
        &self.stream_key
    }

    /// Force close the connection. Calling this function is not required
    /// for cleanup. it is useful when you can't drop the connection because
    /// you are blocked in iterator loop.
    pub fn close(&self) {
        self.shutdown_condition.mark_for_shutdown();
    }

    /// Ask the connected publisher to reconnect, per E-RTMP
    /// `NetConnection.Connect.ReconnectRequest` (Enhanced RTMP v2, "Reconnect Request").
    ///
    /// - `tc_url`: optional absolute or relative URI reference. When `None`, the publisher
    ///   reconnects to the current `tcUrl`. A relative reference is resolved against the
    ///   current `tcUrl` by the publisher.
    /// - `description`: optional human-readable description.
    ///
    /// The server thread sends the request only if the publisher advertised
    /// `capsEx.Reconnect` during the connect handshake. After dispatching, the server
    /// continues processing incoming media until the publisher disconnects, as required
    /// by the spec.
    pub fn request_reconnect(&self, tc_url: Option<&str>, description: Option<&str>) {
        let _ = self.command_sender.send(ServerCommand::RequestReconnect {
            tc_url: tc_url.map(str::to_string),
            description: description.map(str::to_string),
        });
    }
}

impl Drop for RtmpServerConnection {
    fn drop(&mut self) {
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl Iterator for &RtmpServerConnection {
    type Item = RtmpEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}
