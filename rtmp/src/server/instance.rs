use std::{
    sync::{Arc, Mutex, Weak},
    thread::JoinHandle,
};

use crossbeam_channel::Sender;

use crate::{
    OnConnectionCallback, RtmpServerConnection, RtmpServerConnectionError, ServerConfig,
    server::listener_thread::start_listener_thread, utils::ShutdownCondition,
};

pub struct RtmpServer(Arc<Mutex<ServerInstance>>);

impl RtmpServer {
    pub(super) fn new(config: ServerConfig, conn_sender: Sender<RtmpServerConnection>) -> Self {
        Self(Arc::new(Mutex::new(ServerInstance {
            config,
            shutdown_condition: ShutdownCondition::default(),
            conn_sender,
        })))
    }

    pub fn config(&self) -> ServerConfig {
        self.0.lock().unwrap().config.clone()
    }

    pub fn start(
        config: ServerConfig,
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
}

struct ServerInstance {
    config: ServerConfig,
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
    pub fn new(server: &ServerHandle) -> Option<Arc<Mutex<Self>>> {
        let server = server.0.upgrade()?;
        let guard = server.lock().unwrap();
        Some(Arc::new(Mutex::new(Self {
            shutdown_condition: guard.shutdown_condition.child_condition(),
            conn_sender: guard.conn_sender.clone(),
            thread_handle: None,
        })))
    }

    pub fn send_connection(
        &self,
        conn: RtmpServerConnection,
    ) -> Result<(), RtmpServerConnectionError> {
        self.conn_sender
            .send(conn)
            .map_err(|_| RtmpServerConnectionError::ShutdownInProgress)?;
        Ok(())
    }
}
