use std::sync::{Arc, Mutex, MutexGuard};

use axum::response::IntoResponse;
use compositor_pipeline::{error::InitPipelineError, pipeline};
use compositor_render::EventLoop;

use serde::Serialize;
use tokio::runtime::Runtime;

use crate::config::Config;

pub type Pipeline = compositor_pipeline::Pipeline;

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
        bearer_token: String,
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
        let mut options: pipeline::Options = (&config).into();
        options.tokio_rt = Some(runtime);
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

impl From<&Config> for pipeline::Options {
    fn from(val: &Config) -> Self {
        pipeline::Options {
            queue_options: val.queue_options,
            stream_fallback_timeout: val.stream_fallback_timeout,
            web_renderer: val.web_renderer,
            force_gpu: val.force_gpu,
            download_root: val.download_root.clone(),
            mixing_sample_rate: val.mixing_sample_rate,
            stun_servers: val.stun_servers.clone(),
            wgpu_features: val.required_wgpu_features,
            wgpu_ctx: None,
            load_system_fonts: Some(val.load_system_fonts),
            start_whip_whep: val.start_whip_whep,
            whip_whep_server_port: val.whip_whep_server_port,
            tokio_rt: None,
            rendering_mode: val.rendering_mode,
        }
    }
}
