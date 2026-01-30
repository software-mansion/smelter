use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use rtmp::{RtmpError, RtmpServer, ServerConfig};
use tracing::warn;

use crate::{
    pipeline::rtmp::rtmp_input::{handle_on_connection, state::RtmpInputsState},
    prelude::*,
};

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
    ctx: Arc<PipelineCtx>,
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

    let on_connection = Box::new(move |conn| {
        handle_on_connection(ctx.clone(), inputs.clone(), conn);
    });

    let mut last_error: Option<RtmpError> = None;
    for _ in 0..5 {
        match RtmpServer::start(config.clone(), on_connection.clone()) {
            Ok(server) => return Ok(server),
            Err(err) => {
                warn!("Failed to start RTMP server. Retrying ...");
                last_error = Some(err)
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }
    Err(InitPipelineError::RtmpServerInitError(last_error.unwrap()))
}
