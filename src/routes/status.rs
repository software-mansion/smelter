use std::time::Duration;

use axum::{extract::State, response::IntoResponse};
use compositor_pipeline::pipeline::{input::Input, output::Output};
use compositor_render::RenderingMode;
use serde::Serialize;
use serde_json::json;

use crate::error::ApiError;

use super::ApiState;

#[derive(Serialize)]
struct InputInfo {
    input_id: String,
    input_type: String,
}

#[derive(Serialize)]
struct OutputInfo {
    output_id: String,
    output_type: String,
}

#[derive(Serialize)]
struct WebRendererConfig {
    enable: bool,
    enable_gpu: bool,
}

#[derive(Serialize)]
struct QueueOptions {
    default_buffer_duration: Duration,
    ahead_of_time_processing: bool,
    output_framerate: Framerate,
    run_late_scheduled_events: bool,
    never_drop_output_frames: bool,
}

#[derive(Serialize)]
struct Framerate {
    num: u32,
    den: u32,
}

pub(super) async fn status_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let pipeline = state.pipeline.lock().unwrap();
    let pipeline_ctx = pipeline.ctx();

    let inputs: Vec<InputInfo> = pipeline
        .inputs()
        .map(|(id, input)| {
            let input_type = match &input.input {
                Input::Rtp(_) => "rtp",
                Input::Mp4(_) => "mp4",
                Input::Whip(_) => "whip",
                #[cfg(feature = "decklink")]
                Input::DeckLink(_) => "decklink",
                Input::RawDataInput => "raw data",
            };
            InputInfo {
                input_id: id.to_string(),
                input_type: input_type.to_string(),
            }
        })
        .collect();

    let outputs: Vec<OutputInfo> = pipeline
        .outputs()
        .map(|(id, output)| {
            let output_type = match &output.output {
                Output::Rtp { .. } => "rtp",
                Output::Rtmp { .. } => "rtmp",
                Output::Mp4 { .. } => "mp4",
                Output::Whip { .. } => "whip",
                Output::EncodedData { .. } => "encoded data",
                Output::RawData { .. } => "raw data",
            };
            OutputInfo {
                output_id: id.to_string(),
                output_type: output_type.to_string(),
            }
        })
        .collect();

    let state_queue_options = state.config.queue_options;
    let queue_options = QueueOptions {
        default_buffer_duration: state_queue_options.default_buffer_duration,
        ahead_of_time_processing: state_queue_options.ahead_of_time_processing,
        output_framerate: Framerate {
            num: state_queue_options.output_framerate.num,
            den: state_queue_options.output_framerate.den,
        },
        run_late_scheduled_events: state_queue_options.run_late_scheduled_events,
        never_drop_output_frames: state_queue_options.never_drop_output_frames,
    };

    let state_web_renderer = state.config.web_renderer;
    let web_renderer = WebRendererConfig {
        enable: state_web_renderer.enable,
        enable_gpu: state_web_renderer.enable_gpu,
    };

    let rendering_mode = match state.config.rendering_mode {
        RenderingMode::GpuOptimized => "GPU optimized",
        RenderingMode::CpuOptimized => "CPU optimized",
        RenderingMode::WebGl => "WebGL",
    };

    Ok(axum::Json(json!({
        "instance_id": state.config.instance_id,
        "api_port": state.config.api_port,
        "stream_fallback_timeout": state.config.stream_fallback_timeout,
        "download_root": state.config.download_root,
        "web_renderer": web_renderer,
        "force_gpu": state.config.force_gpu,
        "queue_options": queue_options,
        "mixing_sample_rate": state.config.mixing_sample_rate,
        "required_wgpu_features": state.config.required_wgpu_features,
        "load_system_fonts": state.config.load_system_fonts,
        "whip_whep_server_port": state.config.whip_whep_server_port,
        "start_whip_whep": state.config.start_whip_whep,
        "rendering_mode": rendering_mode,
        "stun_servers": pipeline_ctx.stun_servers,
        "inputs": inputs,
        "outputs": outputs
    }))
    .into_response())
}
