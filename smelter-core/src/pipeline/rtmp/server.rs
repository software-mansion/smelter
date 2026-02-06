use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use rtmp::{RtmpConnection, RtmpError, RtmpServer, ServerConfig};
use smelter_render::error::ErrorStack;
use tracing::{error, warn};

use crate::pipeline::rtmp::rtmp_input::{
    connection::{RtmpConnectionOptions, start_connection_thread},
    state::RtmpInputsState,
};

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
        if let Err(err) = handle_incoming_connection(ctx.clone(), inputs.clone(), conn) {
            error!(
                "Failed to handle incoming RTMP connection: {}",
                ErrorStack::new(&err).into_string()
            );
        }
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

fn handle_incoming_connection(
    ctx: Arc<PipelineCtx>,
    inputs: RtmpInputsState,
    conn: RtmpConnection,
) -> Result<(), RtmpServerError> {
    let input_ref = inputs.find_by_app_stream_key(&conn.app, &conn.stream_key)?;

    if inputs.has_active_connection(&input_ref) {
        return Err(RtmpServerError::ConnectionAlreadyActive(
            input_ref.id().clone(),
        ));
    }

    let options = inputs.get_with(&input_ref, |input| {
        Ok(RtmpConnectionOptions {
            app: input.app.clone(),
            stream_key: input.stream_key.clone(),
            frame_sender: input.frame_sender.clone(),
            samples_sender: input.input_samples_sender.clone(),
            video_decoders: input.video_decoders.clone(),
            buffer: input.buffer.clone(),
        })
    })?;

    let handle = start_connection_thread(ctx, input_ref.clone(), conn.receiver, options);
    inputs.set_connection_handle(&input_ref, handle)?;

    Ok(())
}
