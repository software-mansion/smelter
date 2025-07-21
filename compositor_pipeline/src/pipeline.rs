use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::runtime::Runtime;

use compositor_render::{
    web_renderer::WebRendererInitOptions, Framerate, RenderingMode, WgpuFeatures,
};

use crate::prelude::*;
use crate::{
    event::EventEmitter, graphics_context::GraphicsContext, pipeline::webrtc::WhipWhepPipelineState,
};

mod decoder;
mod encoder;
mod input;
mod output;
mod resampler;
mod rtp;
mod webrtc;

mod instance;
mod pipeline_input;
mod pipeline_output;
mod types;

pub use instance::Pipeline;

#[derive(Debug)]
pub struct PipelineOptions {
    pub queue_options: QueueOptions,
    pub stream_fallback_timeout: Duration,
    pub web_renderer: WebRendererInitOptions,
    pub force_gpu: bool,
    pub download_root: PathBuf,
    pub mixing_sample_rate: u32,
    pub stun_servers: Arc<Vec<String>>,
    pub wgpu_features: WgpuFeatures,
    pub load_system_fonts: Option<bool>,
    pub wgpu_ctx: Option<GraphicsContext>,
    pub whip_whep_server_port: u16,
    pub start_whip_whep: bool,
    pub tokio_rt: Option<Arc<Runtime>>,
    pub rendering_mode: RenderingMode,
}

pub const DEFAULT_BUFFER_DURATION: Duration = Duration::from_millis(16 * 5); // about 5 frames at 60 fps

#[derive(Debug, Clone, Copy)]
pub struct QueueOptions {
    pub default_buffer_duration: Duration,
    pub ahead_of_time_processing: bool,
    pub output_framerate: Framerate,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
}

#[derive(Clone)]
pub struct PipelineCtx {
    pub queue_sync_time: Instant,
    pub mixing_sample_rate: u32,
    pub output_framerate: Framerate,
    pub stun_servers: Arc<Vec<String>>,
    pub download_dir: Arc<Path>,
    pub graphics_context: GraphicsContext,
    event_emitter: Arc<EventEmitter>,
    tokio_rt: Arc<Runtime>,
    whip_whep_state: Option<Arc<WhipWhepPipelineState>>,
}

impl std::fmt::Debug for PipelineCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineCtx")
            .field("mixing_sample_rate", &self.mixing_sample_rate)
            .field("output_framerate", &self.output_framerate)
            .field("download_dir", &self.download_dir)
            .field("event_emitter", &self.event_emitter)
            .finish()
    }
}
