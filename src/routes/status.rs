use std::{path::Path, sync::Arc};

use axum::{extract::State, response::IntoResponse};
use compositor_pipeline::{InputProtocolKind, OutputProtocolKind};
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
struct InstanceConfiguration {
    api_port: u16,

    output_framerate: f64,
    mixing_sample_rate: u32,

    ahead_of_time_processing: bool,
    never_drop_output_frames: bool,
    run_late_scheduled_events: bool,

    download_root: Arc<Path>,

    web_renderer_enable: bool,
    web_renderer_enable_gpu: bool,

    whip_whep_server_port: u16,
    whip_whep_enable: bool,
    whip_whep_stun_servers: Arc<Vec<String>>,

    rendering_mode: &'static str,
}

pub(super) async fn status_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, ApiError> {
    let pipeline = state.pipeline.lock().unwrap();

    let inputs: Vec<InputInfo> = pipeline
        .inputs()
        .map(|(id, input)| {
            let input_type = match &input.protocol {
                InputProtocolKind::Rtp => "rtp",
                InputProtocolKind::Mp4 => "mp4",
                InputProtocolKind::Whip => "whip",
                InputProtocolKind::Hls => "hls",
                InputProtocolKind::DeckLink => "decklink",
                InputProtocolKind::RawDataChannel => "raw_data",
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
            let output_type = match &output.protocol {
                OutputProtocolKind::Rtp => "rtp",
                OutputProtocolKind::Rtmp => "rtmp",
                OutputProtocolKind::Mp4 => "mp4",
                OutputProtocolKind::Whip => "whip",
                OutputProtocolKind::Hls => "hls",
                OutputProtocolKind::EncodedDataChannel => "encoded_data",
                OutputProtocolKind::RawDataChannel => "raw_data",
            };
            OutputInfo {
                output_id: id.to_string(),
                output_type: output_type.to_string(),
            }
        })
        .collect();

    let output_framerate = state.config.output_framerate;
    let configuration = InstanceConfiguration {
        api_port: state.config.api_port,
        whip_whep_server_port: state.config.whip_whep_server_port,
        output_framerate: output_framerate.num as f64 / output_framerate.den as f64,
        mixing_sample_rate: state.config.mixing_sample_rate,
        ahead_of_time_processing: state.config.ahead_of_time_processing,
        never_drop_output_frames: state.config.never_drop_output_frames,
        run_late_scheduled_events: state.config.run_late_scheduled_events,
        download_root: state.config.download_root,
        whip_whep_stun_servers: state.config.whip_whep_stun_servers,
        web_renderer_enable: state.config.web_renderer_enable,
        web_renderer_enable_gpu: state.config.web_renderer_gpu_enable,
        whip_whep_enable: state.config.whip_whep_enable,
        rendering_mode: match state.config.rendering_mode {
            RenderingMode::GpuOptimized => "gpu_optimized",
            RenderingMode::CpuOptimized => "cpu_optimized",
            RenderingMode::WebGl => "webgl",
        },
    };

    Ok(axum::Json(json!({
        "instance_id": state.config.instance_id,
        "configuration": configuration,
        "inputs": inputs,
        "outputs": outputs
    }))
    .into_response())
}
