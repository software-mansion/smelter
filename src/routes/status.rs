use std::sync::Arc;

use axum::extract::State;
use smelter_api::{InputStatus, InstanceConfiguration, InstanceStatus, OutputStatus};
use smelter_core::stats::StatsReport;
use smelter_render::RenderingMode;

use crate::{error::ApiError, routes::Json};

use super::ApiState;

#[utoipa::path(
    get,
    path = "/status",
    operation_id = "get_status",
    responses(
        (status = 200, description = "Instance status fetched successfully.", body = InstanceStatus),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["metadata_request"],
)]
pub async fn status_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<InstanceStatus>, ApiError> {
    let pipeline = state.pipeline()?;
    let pipeline = pipeline.lock().unwrap();

    let inputs = pipeline
        .inputs()
        .map(|(id, input)| InputStatus {
            input_id: id.to_string(),
            input_type: input.protocol.into(),
        })
        .collect();

    let outputs = pipeline
        .outputs()
        .map(|(id, output)| OutputStatus {
            output_id: id.to_string(),
            output_type: output.protocol.into(),
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
        download_root: state.config.download_root.clone(),
        webrtc_stun_servers: state.config.webrtc_stun_servers.clone(),
        web_renderer_enable: state.config.web_renderer_enable,
        web_renderer_gpu_enable: state.config.web_renderer_gpu_enable,
        whip_whep_enable: state.config.whip_whep_enable,
        rendering_mode: match state.config.rendering_mode {
            RenderingMode::GpuOptimized => "gpu_optimized",
            RenderingMode::CpuOptimized => "cpu_optimized",
            RenderingMode::WebGl => "webgl",
        },
    };

    Ok(Json(InstanceStatus {
        instance_id: state.config.instance_id.clone(),
        configuration,
        inputs,
        outputs,
    }))
}

#[utoipa::path(
    get,
    path = "/stats",
    operation_id = "get_stats",
    responses(
        (status = 200, description = "Statistics for inputs and outputs fetched successfully.", body = StatsReport),
        (status = 500, description = "Internal server error.", body = ApiError),
    ),
    tags = ["metadata_request"],
)]
pub async fn stats_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<StatsReport>, ApiError> {
    let pipeline = state.pipeline()?;
    Ok(Json(pipeline.lock().unwrap().stats()))
}
