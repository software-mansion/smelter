use rtmp::RtmpConnection;
use std::sync::Arc;
use tracing::{Level, error, info, span};

use crate::{
    pipeline::rtmp::rtmp_input::{connection_state::RtmpConnectionState, state::RtmpInputsState},
    prelude::*,
};

pub(crate) fn handle_on_connection(
    ctx: Arc<PipelineCtx>,
    inputs: RtmpInputsState,
    conn: RtmpConnection,
) {
    let app = conn.app;
    let stream_key = conn.stream_key;
    let receiver = conn.receiver;

    let input_ref = match inputs.find_by_app_stream_key(&app, &stream_key) {
        Ok(state) => state,
        Err(err) => {
            error!(?err, "Failed to find input");
            return;
        }
    };

    if inputs.has_active_connection(&input_ref) {
        error!(
            ?app,
            ?stream_key,
            input_id=?input_ref.id(),
            "Rejecting connection. Input stream is already active"
        );
        return;
    }

    let (input_buffer, frame_sender, samples_sender, video_decoders) =
        match inputs.get_with(&input_ref, |input| {
            Ok((
                input.buffer.clone(),
                input.frame_sender.clone(),
                input.input_samples_sender.clone(),
                input.video_decoders.clone(),
            ))
        }) {
            Ok(state) => state,
            Err(err) => {
                error!(?err, ?app, ?stream_key, "Failed to retrieve input buffer");
                return;
            }
        };

    let input_ref_clone = input_ref.clone();
    let handle = std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_ref}"))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "RTMP thread",
                input_id = input_ref_clone.id().to_string(),
                app = app.to_string(),
                stream_key = stream_key.to_string(),
            )
            .entered();
            let mut connection_state = RtmpConnectionState::new(
                ctx,
                input_ref_clone,
                frame_sender,
                samples_sender,
                video_decoders,
                input_buffer,
            );
            info!("RTMP stream connection opened");

            while let Ok(rtmp_event) = receiver.recv() {
                connection_state.handle_rtmp_event(rtmp_event);
            }

            info!("RTMP stream connection closed");
        })
        .unwrap();

    if let Err(err) = inputs.set_connection_handle(&input_ref, handle) {
        error!(?err, "Failed to store connection handle");
    }
}
