use rtmp::{RtmpConnection, RtmpEvent};
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    pipeline::rtmp::rtmp_input::{
        RtmpConnectionContext,
        process_audio::{process_audio, process_audio_config},
        process_video::{process_video, process_video_config},
        state::RtmpInputsState,
        stream_state::RtmpStreamState,
    },
    prelude::*,
};

pub(crate) fn handle_on_connection(
    ctx: Arc<PipelineCtx>,
    inputs: RtmpInputsState,
    conn: RtmpConnection,
) {
    let RtmpConnection {
        app,
        stream_key,
        receiver,
    } = conn;

    let (input_ref, input_state) = match inputs.get_input_state(app.clone(), stream_key.clone()) {
        Ok(state) => state,
        Err(err) => {
            error!(?err, "No input with provided app, stream_key found");
            return;
        }
    };

    let session_ctx = RtmpConnectionContext::new(
        ctx.clone(),
        inputs,
        input_ref,
        app.clone(),
        stream_key.clone(),
    );

    let input_buffer = input_state.buffer.clone();

    std::thread::spawn(move || {
        let mut stream_state = RtmpStreamState::new(&session_ctx.ctx, input_buffer);
        info!(app = ?session_ctx.app, stream_key = ?session_ctx.stream_key, "Stream connection opened");

        while let Ok(media_data) = receiver.recv() {
            process_media(&session_ctx, &mut stream_state, media_data);
        }

        info!(app = ?session_ctx.app, stream_key = ?session_ctx.stream_key, "Stream connection closed");
    });
}

fn process_media(
    ctx: &RtmpConnectionContext,
    stream_state: &mut RtmpStreamState,
    rtmp_event: RtmpEvent,
) {
    match rtmp_event {
        RtmpEvent::VideoConfig(config) => process_video_config(ctx, config),
        RtmpEvent::AudioConfig(config) => process_audio_config(ctx, config),
        RtmpEvent::Video(data) => process_video(ctx, stream_state, data),
        RtmpEvent::Audio(data) => process_audio(ctx, stream_state, data),
        RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
    }
}
