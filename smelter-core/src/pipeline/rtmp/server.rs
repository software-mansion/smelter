use std::sync::{Arc, Mutex};

use rtmp::{RtmpConnection, RtmpServer, ServerConfig};
use tracing::error;

use super::state::RtmpInputsState;

use crate::prelude::*;

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

pub fn spawn_rtmp_server(
    state: &RtmpPipelineState,
) -> Result<Arc<Mutex<RtmpServer>>, InitPipelineError> {
    let port = state.port;
    let inputs = state.inputs.clone();

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

    // TODO add retry
    let server =
        RtmpServer::start(config, on_connection).map_err(InitPipelineError::RtmpServerInitError)?;

    Ok(server)
}
