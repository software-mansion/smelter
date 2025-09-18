use std::sync::{Arc, Mutex};

use axum::response::IntoResponse;
use smelter_core::{
    error::InitPipelineError, Pipeline, PipelineOptions, PipelineWgpuOptions,
    PipelineWhipWhepServerOptions,
};
use smelter_render::web_renderer::{ChromiumContext, ChromiumContextInitError};

use reqwest::StatusCode;
use serde::Serialize;
use tokio::runtime::Runtime;

use crate::{config::Config, error::ApiError};

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum Response {
    Ok {},
    RegisteredPort {
        port: Option<u16>,
    },
    RegisteredMp4 {
        video_duration_ms: Option<u64>,
        audio_duration_ms: Option<u64>,
    },
    BearerToken {
        bearer_token: Arc<str>,
    },
}

impl IntoResponse for Response {
    fn into_response(self) -> axum::response::Response {
        axum::Json(self).into_response()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiStateInitError {
    #[error(transparent)]
    PipelineInit(#[from] InitPipelineError),

    #[error(transparent)]
    ChromiumContextInit(#[from] ChromiumContextInitError),
}

pub struct ApiState {
    pub pipeline: Mutex<Option<Arc<Mutex<Pipeline>>>>,
    pub config: Config,
    pub chromium_context: Option<Arc<ChromiumContext>>,
    pub runtime: Arc<Runtime>,
}

impl ApiState {
    pub fn new(config: Config, runtime: Arc<Runtime>) -> Result<Arc<ApiState>, ApiStateInitError> {
        let chromium_context = match config.web_renderer_enable && cfg!(feature = "web_renderer") {
            true => Some(ChromiumContext::new(
                config.output_framerate,
                config.web_renderer_gpu_enable,
            )?),
            false => None,
        };
        let options = pipeline_options_from_config(&config, &runtime, &chromium_context);
        let pipeline = Pipeline::new(options)?;
        Ok(Arc::new(ApiState {
            pipeline: Mutex::new(Some(Arc::new(Mutex::new(pipeline)))),
            config,
            runtime,
            chromium_context,
        }))
    }

    pub fn pipeline(&self) -> Result<Arc<Mutex<Pipeline>>, ApiError> {
        match self.pipeline.lock().unwrap().clone() {
            Some(pipeline) => Ok(pipeline),
            None => Err(ApiError {
                error_code: "PIPELINE_DOWN",
                message: "Pipeline reset failed. Pipeline is down".to_string(),
                stack: Vec::new(),
                http_status_code: StatusCode::INTERNAL_SERVER_ERROR,
            }),
        }
    }

    pub fn reset(&self) -> Result<(), ApiError> {
        let mut guard = self.pipeline.lock().unwrap();
        guard.take();

        let options =
            pipeline_options_from_config(&self.config, &self.runtime, &self.chromium_context);
        let pipeline = Arc::new(Mutex::new(Pipeline::new(options)?));
        *guard = Some(pipeline);
        Ok(())
    }
}

pub fn pipeline_options_from_config(
    opt: &Config,
    tokio_rt: &Arc<Runtime>,
    chromium_context: &Option<Arc<ChromiumContext>>,
) -> PipelineOptions {
    PipelineOptions {
        stream_fallback_timeout: opt.stream_fallback_timeout,
        download_root: opt.download_root.clone(),
        default_buffer_duration: opt.default_buffer_duration,

        load_system_fonts: opt.load_system_fonts,
        ahead_of_time_processing: opt.ahead_of_time_processing,
        run_late_scheduled_events: opt.run_late_scheduled_events,
        never_drop_output_frames: opt.never_drop_output_frames,

        mixing_sample_rate: opt.mixing_sample_rate,
        output_framerate: opt.output_framerate,

        rendering_mode: opt.rendering_mode,
        tokio_rt: Some(tokio_rt.clone()),

        chromium_context: chromium_context.clone(),
        wgpu_options: PipelineWgpuOptions::Options {
            features: opt.wgpu_required_features,
            force_gpu: opt.wgpu_force_gpu,
        },

        whip_whep_stun_servers: opt.whip_whep_stun_servers.clone(),
        whip_whep_server: match opt.whip_whep_enable {
            true => PipelineWhipWhepServerOptions::Enable {
                port: opt.whip_whep_server_port,
            },
            false => PipelineWhipWhepServerOptions::Disable,
        },
    }
}
