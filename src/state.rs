use std::sync::{Arc, Mutex, MutexGuard};

use axum::response::IntoResponse;
use compositor_pipeline::{
    error::InitPipelineError, Pipeline, PipelineOptions, PipelineWgpuOptions,
    PipelineWhipWhepServerOptions,
};
use compositor_render::{web_renderer::WebRendererInitOptions, EventLoop};

use serde::Serialize;
use tokio::runtime::Runtime;

use crate::config::Config;

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

#[derive(Clone)]
pub struct ApiState {
    pub pipeline: Arc<Mutex<Pipeline>>,
    pub config: Config,
}

impl ApiState {
    pub fn new(
        config: Config,
        runtime: Arc<Runtime>,
    ) -> Result<(ApiState, Arc<dyn EventLoop>), InitPipelineError> {
        let options = pipeline_options_from_config(&config, runtime);
        let (pipeline, event_loop) = Pipeline::new(options)?;
        Ok((
            ApiState {
                pipeline: Mutex::new(pipeline).into(),
                config,
            },
            event_loop,
        ))
    }

    pub(crate) fn pipeline(&self) -> MutexGuard<'_, Pipeline> {
        self.pipeline.lock().unwrap()
    }
}

pub fn pipeline_options_from_config(opt: &Config, tokio_rt: Arc<Runtime>) -> PipelineOptions {
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
        tokio_rt: Some(tokio_rt),

        web_renderer: match opt.web_renderer_enable {
            true => WebRendererInitOptions::Enable {
                enable_gpu: opt.web_renderer_gpu_enable,
            },
            false => WebRendererInitOptions::Disable,
        },
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
