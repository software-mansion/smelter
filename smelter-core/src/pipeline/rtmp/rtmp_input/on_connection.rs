use rtmp::{RtmpConnection, RtmpEvent};
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    pipeline::rtmp::rtmp_input::{
        input_state::RtmpInputsState,
        process_audio::{process_audio, process_audio_config},
        process_video::{process_video, process_video_config},
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

    let (input_ref, input_state) =
        match inputs.find_by_app_stream_key(app.clone(), stream_key.clone()) {
            Ok(state) => state,
            Err(err) => {
                error!(?err, "No input with provided app, stream_key found");
                return;
            }
        };

    let input_buffer = input_state.buffer.clone();

    std::thread::spawn(move || {
        let mut stream_state = RtmpStreamState::new(&ctx, input_buffer);
        info!(?app, ?stream_key, "Stream connection opened");

        while let Ok(rtmp_event) = receiver.recv() {
            handle_rtmp_event(&ctx, &inputs, &input_ref, &mut stream_state, rtmp_event);
        }

        info!(?app, ?stream_key, "Stream connection closed");
    });
}

fn handle_rtmp_event(
    ctx: &Arc<PipelineCtx>,
    inputs: &RtmpInputsState,
    input_ref: &Ref<InputId>,
    stream_state: &mut RtmpStreamState,
    rtmp_event: RtmpEvent,
) {
    match rtmp_event {
        RtmpEvent::VideoConfig(config) => process_video_config(ctx, inputs, input_ref, config),
        RtmpEvent::AudioConfig(config) => process_audio_config(ctx, inputs, input_ref, config),
        RtmpEvent::Video(data) => process_video(inputs, input_ref, stream_state, data),
        RtmpEvent::Audio(data) => process_audio(inputs, input_ref, stream_state, data),
        RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
    }
}
