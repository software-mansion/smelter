use std::{
    sync::{Arc, Mutex, Weak},
    thread::JoinHandle,
};

use crossbeam_channel::{Receiver, Sender};

use crate::{
    OnConnectionCallback, RtmpEvent, RtmpServerConfig, RtmpServerConnection,
    RtmpServerConnectionError, server::listener_thread::start_listener_thread,
    utils::ShutdownCondition,
};

pub struct RtmpServer(Arc<Mutex<ServerInstance>>);

impl RtmpServer {
    pub(super) fn new(config: RtmpServerConfig, conn_sender: Sender<RtmpServerConnection>) -> Self {
        Self(Arc::new(Mutex::new(ServerInstance {
            config,
            shutdown_condition: ShutdownCondition::default(),
            conn_sender,
            listener_thread: None,
            on_connection_thread: None,
        })))
    }

    pub(super) fn set_threads(
        &self,
        listener_thread: JoinHandle<()>,
        on_connection_thread: JoinHandle<()>,
    ) {
        let mut guard = self.0.lock().unwrap();
        guard.listener_thread = Some(listener_thread);
        guard.on_connection_thread = Some(on_connection_thread);
    }

    pub fn config(&self) -> RtmpServerConfig {
        self.0.lock().unwrap().config.clone()
    }

    pub fn start(
        config: RtmpServerConfig,
        on_connection: OnConnectionCallback,
    ) -> Result<Self, std::io::Error> {
        start_listener_thread(config, on_connection)
    }

    pub fn shutdown(&self) {
        let guard = self.0.lock().unwrap();
        guard.shutdown_condition.mark_for_shutdown();
    }

    pub(super) fn handle(&self) -> ServerHandle {
        ServerHandle(Arc::downgrade(&self.0))
    }
}

pub(super) struct ServerHandle(Weak<Mutex<ServerInstance>>);

impl ServerHandle {
    pub fn should_stop_server(&self) -> bool {
        let Some(server) = self.0.upgrade() else {
            return true;
        };
        server.lock().unwrap().shutdown_condition.should_close()
    }

    pub fn upgrade(&self) -> Option<RtmpServer> {
        self.0.upgrade().map(RtmpServer)
    }
}

struct ServerInstance {
    config: RtmpServerConfig,
    conn_sender: Sender<RtmpServerConnection>,
    shutdown_condition: ShutdownCondition,
    /// Listener thread (accept loop). Joined on drop so it doesn't outlive
    /// the `RtmpServer` while still holding `Arc<PipelineCtx>` clones via
    /// the on-connection callback.
    listener_thread: Option<JoinHandle<()>>,
    /// Thread that runs the on-connection callback. Joined on drop for the
    /// same reason as `listener_thread`.
    on_connection_thread: Option<JoinHandle<()>>,
}

impl Drop for ServerInstance {
    fn drop(&mut self) {
        self.shutdown_condition.mark_for_shutdown();
        // Drop the conn_sender so the on-connection thread's
        // `conn_receiver.into_iter()` ends.
        // (The field will drop as part of the struct, but we want it gone
        // before we join the on_connection_thread below.)
        // Note: we can't easily move it out of `&mut self`, so we rely on
        // `shutdown_condition` plus the listener exiting to close the
        // sender via the listener thread's drop of its clone.
        if let Some(handle) = self.listener_thread.take()
            && let Err(err) = handle.join()
        {
            tracing::error!(?err, "RTMP listener thread panicked during join");
        }
        if let Some(handle) = self.on_connection_thread.take()
            && let Err(err) = handle.join()
        {
            tracing::error!(?err, "RTMP on-connection thread panicked during join");
        }
    }
}

pub(super) struct ServerConnectionCtx {
    pub shutdown_condition: ShutdownCondition,
    pub conn_sender: Sender<RtmpServerConnection>,
    pub thread_handle: Option<JoinHandle<()>>,
}

impl ServerConnectionCtx {
    pub fn new(server: &RtmpServer) -> Arc<Mutex<Self>> {
        let guard = server.0.lock().unwrap();
        Arc::new(Mutex::new(Self {
            shutdown_condition: guard.shutdown_condition.child_condition(),
            conn_sender: guard.conn_sender.clone(),
            thread_handle: None,
        }))
    }

    pub fn send_connection(
        &self,
        app: Arc<str>,
        stream_key: Arc<str>,
        receiver: Receiver<RtmpEvent>,
    ) -> Result<(), RtmpServerConnectionError> {
        let conn = RtmpServerConnection {
            app,
            stream_key,
            receiver,
            shutdown_condition: self.shutdown_condition.clone(),
        };
        self.conn_sender
            .send(conn)
            .map_err(|_| RtmpServerConnectionError::ShutdownInProgress)?;
        Ok(())
    }
}
