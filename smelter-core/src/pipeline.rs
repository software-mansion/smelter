use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::runtime::Runtime;

use smelter_render::{Framerate, RenderingMode, WgpuFeatures, web_renderer::ChromiumContext};

use crate::{
    event::EventEmitter, graphics_context::GraphicsContext,
    pipeline::webrtc::WhipWhepPipelineState, stats::StatsSender,
};

use crate::prelude::*;

mod decoder;
mod encoder;
mod resampler;

mod ffmpeg_utils;

#[cfg(feature = "decklink")]
mod decklink;

#[cfg(target_os = "linux")]
mod v4l2;

mod channel;
mod hls;
mod mp4;
mod rtmp;
mod rtp;
mod webrtc;

mod input;
mod instance;
mod output;
mod utils;

pub use instance::Pipeline;

#[cfg(target_os = "linux")]
pub use v4l2::{V4l2DeviceInfo, V4l2FormatInfo, V4l2ResolutionInfo, list_v4l2_devices};

#[derive(Debug)]
pub struct PipelineOptions {
    pub stream_fallback_timeout: Duration,
    pub default_buffer_duration: Duration,

    pub load_system_fonts: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
    pub ahead_of_time_processing: bool,

    pub output_framerate: Framerate,
    pub mixing_sample_rate: u32,

    pub download_root: Arc<Path>,

    pub rendering_mode: RenderingMode,
    pub wgpu_options: PipelineWgpuOptions,
    pub tokio_rt: Option<Arc<Runtime>>,

    /// required for web rendering support
    pub chromium_context: Option<Arc<ChromiumContext>>,

    pub whip_whep_server: PipelineWhipWhepServerOptions,
    pub whip_whep_stun_servers: Arc<Vec<String>>,
}

#[derive(Debug)]
pub enum PipelineWgpuOptions {
    Context(GraphicsContext),
    Options {
        device_id: Option<u32>,
        driver_name: Option<String>,
        features: WgpuFeatures,
        force_gpu: bool,
    },
}

#[derive(Debug)]
pub enum PipelineWhipWhepServerOptions {
    Enable { port: u16 },
    Disable,
}

pub const DEFAULT_BUFFER_DURATION: Duration = Duration::from_millis(16 * 5); // about 5 frames at 60 fps

#[derive(Clone)]
pub(crate) struct PipelineCtx {
    pub queue_sync_point: Instant,
    pub default_buffer_duration: Duration,

    pub mixing_sample_rate: u32,
    pub output_framerate: Framerate,

    pub stun_servers: Arc<Vec<String>>,
    pub download_dir: Arc<Path>,
    pub graphics_context: GraphicsContext,
    pub event_emitter: Arc<EventEmitter>,
    pub stats_sender: StatsSender,
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
