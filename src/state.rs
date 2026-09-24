use std::sync::{Arc, Mutex};

use smelter_core::{
    LateEventPolicy, Pipeline, PipelineMoqServerOptions, PipelineOptions,
    PipelineRtmpServerOptions, PipelineWgpuOptions, PipelineWhipWhepServerOptions, Timestamp,
    error::InitPipelineError, protocols::WebrtcUdpPortStrategy,
};
use smelter_render::{
    error::ErrorStack,
    web_renderer::{ChromiumContext, ChromiumContextInitError},
};

use reqwest::StatusCode;
use tokio::runtime::Runtime;
use tracing::error;

use crate::{config::Config, error::ApiError};

#[derive(Debug, thiserror::Error)]
pub enum ApiStateInitError {
    #[error(transparent)]
    PipelineInit(#[from] InitPipelineError),

    #[error(transparent)]
    ChromiumContextInit(#[from] ChromiumContextInitError),
}

enum PipelineState {
    Running(Arc<Mutex<Pipeline>>),
    Resetting,
    /// Last reset failed.
    Down,
}

pub struct ApiState {
    pipeline: Mutex<PipelineState>,
    pub config: Config,
    pub chromium_context: Option<Arc<ChromiumContext>>,
    pub runtime: Arc<Runtime>,
}

impl ApiState {
    pub fn new(config: Config, runtime: Arc<Runtime>) -> Result<Arc<ApiState>, ApiStateInitError> {
        let chromium_context = match config.web_renderer_enable && cfg!(feature = "web-renderer") {
            true => Some(ChromiumContext::new(
                config.output_framerate,
                config.web_renderer_gpu_enable,
            )?),
            false => None,
        };
        let options = pipeline_options_from_config(&config, &runtime, &chromium_context);
        let pipeline = Arc::new(Mutex::new(Pipeline::new(options)?));
        Ok(Self::with_pipeline(
            config,
            runtime,
            chromium_context,
            pipeline,
        ))
    }

    /// Creates state around an already created pipeline. A reset still creates the new
    /// pipeline from `config`.
    pub fn with_pipeline(
        config: Config,
        runtime: Arc<Runtime>,
        chromium_context: Option<Arc<ChromiumContext>>,
        pipeline: Arc<Mutex<Pipeline>>,
    ) -> Arc<ApiState> {
        Arc::new(ApiState {
            pipeline: Mutex::new(PipelineState::Running(pipeline)),
            config,
            runtime,
            chromium_context,
        })
    }

    pub fn pipeline(&self) -> Result<Arc<Mutex<Pipeline>>, ApiError> {
        match &*self.pipeline.lock().unwrap() {
            PipelineState::Running(pipeline) => Ok(pipeline.clone()),
            PipelineState::Resetting => Err(ApiError::new(
                "PIPELINE_RESETTING",
                "Pipeline reset is in progress.".to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            )),
            PipelineState::Down => Err(ApiError::new(
                "PIPELINE_DOWN",
                "Pipeline reset failed. Pipeline is down".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )),
        }
    }

    /// Runs `action` at `schedule_time`, or immediately when it is not set. Errors from
    /// scheduled actions cannot be returned to the caller, so they are only logged.
    pub fn schedule_or_run<E>(
        &self,
        schedule_time: Option<Timestamp>,
        action: impl FnOnce(&mut Pipeline) -> Result<(), E> + Send + 'static,
    ) -> Result<(), ApiError>
    where
        E: std::error::Error + 'static,
        ApiError: From<E>,
    {
        let pipeline = self.pipeline()?;
        match schedule_time {
            Some(schedule_time) => Pipeline::schedule_event(
                &pipeline,
                schedule_time,
                LateEventPolicy::Default,
                move |pipeline| {
                    if let Err(err) = action(pipeline) {
                        error!(
                            "Error while running scheduled request for pts {}ms: {}",
                            schedule_time.as_millis(),
                            ErrorStack::new(&err).into_string()
                        )
                    }
                },
            ),
            None => action(&mut pipeline.lock().unwrap())?,
        }
        Ok(())
    }

    /// Replaces the pipeline with a new one. The state lock is not held while the new
    /// pipeline is created, so other requests fail with `PIPELINE_RESETTING` instead of
    /// blocking.
    pub fn reset(&self) -> Result<(), ApiError> {
        {
            let mut state = self.pipeline.lock().unwrap();
            if matches!(*state, PipelineState::Resetting) {
                return Err(ApiError::new(
                    "PIPELINE_RESETTING",
                    "Pipeline reset is already in progress.".to_string(),
                    StatusCode::CONFLICT,
                ));
            }
            *state = PipelineState::Resetting;
        }

        let options =
            pipeline_options_from_config(&self.config, &self.runtime, &self.chromium_context);
        let result = Pipeline::new(options);

        let mut state = self.pipeline.lock().unwrap();
        match result {
            Ok(pipeline) => {
                *state = PipelineState::Running(Arc::new(Mutex::new(pipeline)));
                Ok(())
            }
            Err(err) => {
                *state = PipelineState::Down;
                Err(err.into())
            }
        }
    }
}

pub fn pipeline_options_from_config(
    opt: &Config,
    tokio_rt: &Arc<Runtime>,
    chromium_context: &Option<Arc<ChromiumContext>>,
) -> PipelineOptions {
    PipelineOptions {
        stale_frame_timeout: opt.stale_frame_timeout,
        download_root: opt.download_root.clone(),
        default_buffer_duration: opt.default_buffer_duration,

        load_system_fonts: opt.load_system_fonts,
        ahead_of_time_processing: opt.ahead_of_time_processing,
        run_late_scheduled_events: opt.run_late_scheduled_events,
        never_drop_output_frames: opt.never_drop_output_frames,
        side_channel_socket_dir: opt.side_channel_socket_dir.clone(),

        mixing_sample_rate: opt.mixing_sample_rate,
        output_framerate: opt.output_framerate,

        rendering_mode: opt.rendering_mode,
        max_layouts_count: opt.render_max_layouts_count,
        tokio_rt: Some(tokio_rt.clone()),

        chromium_context: chromium_context.clone(),
        wgpu_options: PipelineWgpuOptions::Options {
            device_id: opt.gpu_device_id,
            driver_name: opt.gpu_driver_name.clone(),
            features: opt.wgpu_required_features,
            force_gpu: opt.wgpu_force_gpu,
        },

        webrtc_stun_servers: opt.webrtc_stun_servers.clone(),
        whip_whep_server: match opt.whip_whep_enable {
            true => PipelineWhipWhepServerOptions::Enable {
                port: opt.whip_whep_server_port,
            },
            false => PipelineWhipWhepServerOptions::Disable,
        },
        webrtc_udp_port_strategy: opt.webrtc_udp_port_strategy.clone().map(|s| match s {
            crate::config::WebrtcUdpPortStrategy::PortRange(start, end) => {
                WebrtcUdpPortStrategy::PortRange(start, end)
            }
            crate::config::WebrtcUdpPortStrategy::Mux(port) => WebrtcUdpPortStrategy::Mux(port),
        }),
        webrtc_nat_1to1_ips: opt.webrtc_nat_1to1_ips.clone(),

        rtmp_server: match opt.rtmp_enable {
            true => PipelineRtmpServerOptions::Enable {
                port: opt.rtmp_server_port,
                tls_config: opt.rtmp_tls_config.clone(),
            },
            false => PipelineRtmpServerOptions::Disable,
        },

        moq_server: match opt.moq_enable {
            true => PipelineMoqServerOptions::Enable {
                port: opt.moq_server_port,
                tls_config: opt.moq_tls_config.clone(),
            },
            false => PipelineMoqServerOptions::Disable,
        },

        moq_disable_tls_verification: opt.moq_disable_tls_verification,
    }
}
