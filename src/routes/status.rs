use axum::{extract::State, response::IntoResponse};
use serde_json::{json, Value};

use crate::error::ApiError;

use super::ApiState;
pub(super) async fn status_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let pipeline = state.pipeline.lock().unwrap();
    let pipeline_ctx = pipeline.ctx();

    let inputs: Vec<Value> = pipeline
        .inputs()
        .map(|(id, input)| {
            let input_type = match &input.input {
                compositor_pipeline::pipeline::input::Input::Rtp(_) => "rtp",
                compositor_pipeline::pipeline::input::Input::Mp4(_) => "mp4",
                compositor_pipeline::pipeline::input::Input::Whip(_) => "whip",
                #[cfg(feature = "decklink")]
                compositor_pipeline::pipeline::input::Input::DeckLink(_) => "decklink",
                compositor_pipeline::pipeline::input::Input::RawDataInput => "raw data",
            };
            json!({
                "input_id": id.to_string(),
                "type": input_type
            })
        })
        .collect();

    let outputs: Vec<Value> = pipeline
        .outputs()
        .map(|(id, output)| {
            let output_type = match &output.output {
                compositor_pipeline::pipeline::output::Output::Rtp { .. } => "rtp",
                compositor_pipeline::pipeline::output::Output::Rtmp { .. } => "rtmp",
                compositor_pipeline::pipeline::output::Output::Mp4 { .. } => "mp4",
                compositor_pipeline::pipeline::output::Output::Whip { .. } => "whip",
                compositor_pipeline::pipeline::output::Output::EncodedData { .. } => "encoded data",
                compositor_pipeline::pipeline::output::Output::RawData { .. } => "raw data",
            };
            json!({
                "output_id": id.to_string(),
                "type": output_type
            })
        })
        .collect();

    let web_r = json!({
        "enable": state.config.web_renderer.enable,
        "enable_gpu": state.config.web_renderer.enable_gpu
    });

    let queue_options = json!({
        "default_buffer_duration": state.config.queue_options.default_buffer_duration,
        "ahead_of_time_processing": state.config.queue_options.ahead_of_time_processing,
        "output_framerate": {
            "num": state.config.queue_options.output_framerate.num,
            "den": state.config.queue_options.output_framerate.den,
        },
        "run_late_scheduled_events": state.config.queue_options.run_late_scheduled_events,
        "never_drop_output_frames": state.config.queue_options.never_drop_output_frames
    });

    Ok(axum::Json(json!({
        "instance_id": state.config.instance_id,
        "api_port": state.config.api_port,
        "stream_fallback_timeout": state.config.stream_fallback_timeout,
        "download_root": state.config.download_root,
        "web_renderer": web_r,
        "force_gpu": state.config.force_gpu,
        "queue_options": queue_options,
        "mixing_sample_rate": state.config.mixing_sample_rate,
        "required_wgpu_features": state.config.required_wgpu_features,
        "load_system_fonts": state.config.load_system_fonts,
        "whip_whep_server_port": state.config.whip_whep_server_port,
        "start_whip_whep": state.config.start_whip_whep,
        "rendering_mode": state.config.rendering_mode.to_string(),
        "stun_servers": pipeline_ctx.stun_servers,
        "inputs": inputs,
        "outputs": outputs
    }))
    .into_response())
}
