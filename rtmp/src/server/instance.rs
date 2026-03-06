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
        })))
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
}

impl Drop for RtmpServer {
    fn drop(&mut self) {
        self.shutdown();
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
