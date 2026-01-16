use std::{sync::Arc, thread};

use rtmp::{RtmpConnection, RtmpServer, ServerConfig};
use tokio::sync::oneshot;
use tracing::{error, info};

use super::state::RtmpInputsState;

use crate::prelude::*;

#[derive(Debug)]
pub struct RtmpServerHandle {
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl Drop for RtmpServerHandle {
    fn drop(&mut self) {
        info!("Stopping RTMP server");
        if let Some(sender) = self.shutdown_sender.take()
            && sender.send(()).is_err()
        {
            error!("Cannot send shutdown signal to RTMP server")
        }
    }
}

pub struct RtmpPipelineState {
    pub port: u16,
    pub inputs: RtmpInputsState,
}

impl RtmpPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: RtmpInputsState::default(),
        })
    }
}

pub fn spawn_rtmp_server(state: &RtmpPipelineState) -> Result<RtmpServerHandle, InitPipelineError> {
    let port = state.port;
    let inputs = state.inputs.clone();
    let (shutdown_sender, _shutdown_receiver) = oneshot::channel();

    let config = ServerConfig {
        port,
        use_ssl: false,
        cert_file: None,
        key_file: None,
        ca_cert_file: None,
        client_timeout_secs: 30,
    };

    let on_connection = Box::new(move |conn: RtmpConnection| {
        let inputs = inputs.clone();
        if let Err(err) = inputs.update(conn.url_path, conn.receiver) {
            error!(%err, "Failed to update RTMP input state");
        }
    });

    thread::Builder::new()
        .name("RTMP server".to_string())
        .spawn(move || {
            let server = RtmpServer::new(config, on_connection)?;
            info!(port, "RTMP server starting");
            server.run()
        })
        .map_err(InitPipelineError::RtmpServerInitError)?;

    Ok(RtmpServerHandle {
        shutdown_sender: Some(shutdown_sender),
    })
}
