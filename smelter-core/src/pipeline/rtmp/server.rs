use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use rtmp::{RtmpConnection, RtmpServer, ServerConfig, TlsConfig};
use smelter_render::error::ErrorStack;
use tracing::{error, warn};

use crate::pipeline::rtmp::rtmp_input::{
    connection::{RtmpConnectionOptions, start_connection_thread},
    state::RtmpInputsState,
};

use crate::prelude::*;

pub struct RtmpPipelineState {
    pub port: u16,
    pub tls_cert_file: Option<Arc<str>>,
    pub tls_key_file: Option<Arc<str>>,
    pub inputs: RtmpInputsState,
}

impl RtmpPipelineState {
    pub fn new(
        port: u16,
        tls_cert_file: Option<Arc<str>>,
        tls_key_file: Option<Arc<str>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            port,
            tls_cert_file,
            tls_key_file,
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

    let tls = match (&state.tls_cert_file, &state.tls_key_file) {
        (Some(cert_file), Some(key_file)) => Some(TlsConfig {
            cert_file: cert_file.clone(),
            key_file: key_file.clone(),
        }),
        _ => None,
    };

    let config = ServerConfig {
        port,
        tls,
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

    let mut last_error: Option<std::io::Error> = None;
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
