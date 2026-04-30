use std::{
    path::Path,
    sync::{Arc, Weak},
    time::Duration,
};

use ::rtmp::TlsConfig;
use smelter_render::{
    Framerate, RenderingMode, WgpuCtx, WgpuFeatures, web_renderer::ChromiumContext,
};
use tokio::runtime::Runtime;

use crate::{
    event::EventEmitter,
    graphics_context::GraphicsContext,
    pipeline::{
        rtmp::RtmpPipelineState,
        webrtc::{WebrtcSettingEngineCtx, WhipWhepPipelineState},
    },
    queue::QueueContext,
    stats::StatsSender,
};

use crate::prelude::*;

mod decoder;
mod encoder;

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

pub(crate) mod utils;

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
    pub side_channel_delay: Duration,
    pub side_channel_socket_dir: Option<Arc<Path>>,

    pub output_framerate: Framerate,
    pub mixing_sample_rate: u32,

    pub download_root: Arc<Path>,

    pub rendering_mode: RenderingMode,
    pub wgpu_options: PipelineWgpuOptions,
    pub tokio_rt: Option<Arc<Runtime>>,

    /// required for web rendering support
    pub chromium_context: Option<Arc<ChromiumContext>>,

    pub whip_whep_server: PipelineWhipWhepServerOptions,
    pub webrtc_stun_servers: Arc<Vec<String>>,
    pub webrtc_udp_port_strategy: Option<WebrtcUdpPortStrategy>,
    pub webrtc_nat_1to1_ips: Arc<Vec<String>>,

    pub rtmp_server: PipelineRtmpServerOptions,
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

#[derive(Debug)]
pub enum PipelineRtmpServerOptions {
    Enable {
        port: u16,
        tls_config: Option<TlsConfig>,
    },
    Disable,
}

pub const DEFAULT_BUFFER_DURATION: Duration = Duration::from_millis(16 * 5); // about 5 frames at 60 fps

#[derive(Clone)]
pub(crate) struct PipelineCtx {
    pub queue_ctx: QueueContext,
    pub default_buffer_duration: Duration,

    pub mixing_sample_rate: u32,
    pub output_framerate: Framerate,

    pub download_dir: Arc<Path>,
    pub graphics_context: GraphicsContext,
    pub wgpu_ctx: Arc<WgpuCtx>,
    pub event_emitter: Arc<EventEmitter>,
    pub stats_sender: StatsSender,
    pub webrtc_stun_servers: Arc<Vec<String>>,
    pub webrtc_setting_engine: WebrtcSettingEngineCtx,
    tokio_rt: Arc<Runtime>,
    whip_whep_state: Option<Arc<WhipWhepPipelineState>>,
    rtmp_state: Option<Arc<RtmpPipelineState>>,

    // Must remain the LAST field. Rust drops fields in declaration order, so this
    // probe runs AFTER `graphics_context` and `wgpu_ctx` have destructed — at which
    // point the wgpu device/instance Arcs should have reached refcount 0 if nothing
    // else is holding them.
    #[allow(dead_code)]
    pub(crate) wgpu_drop_probe: WgpuDropProbe,
}

#[derive(Clone)]
pub(crate) struct WgpuDropProbe {
    pub device: Weak<wgpu::Device>,
    pub instance: Weak<wgpu::Instance>,
    pub wgpu_ctx: Weak<WgpuCtx>,
}

impl Drop for WgpuDropProbe {
    fn drop(&mut self) {
        let device_strong = self.device.strong_count();
        let instance_strong = self.instance.strong_count();
        let wgpu_ctx_strong = self.wgpu_ctx.strong_count();
        if device_strong == 0 {
            tracing::error!("DROP wgpu::Device released");
        } else {
            tracing::error!(
                strong = device_strong,
                "WGPU LEAK: wgpu::Device still alive"
            );
        }
        if instance_strong == 0 {
            tracing::error!("DROP wgpu::Instance released");
        } else {
            tracing::error!(
                strong = instance_strong,
                "WGPU LEAK: wgpu::Instance still alive"
            );
        }
        if wgpu_ctx_strong == 0 {
            tracing::error!("DROP WgpuCtx released");
        } else {
            tracing::error!(strong = wgpu_ctx_strong, "WGPU LEAK: WgpuCtx still alive");
        }
    }
}

impl PipelineCtx {
    pub(crate) fn spawn_tracked<F, T>(&self, future: F)
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = self.tokio_rt.spawn(async move {
            let _ = future.await;
        });
        crate::pipeline::utils::async_task::AsyncTaskRegistry::get().register(handle);
    }
}

impl Drop for PipelineCtx {
    fn drop(&mut self) {
        tracing::error!(
            wgpu_device_strong = Arc::strong_count(&self.graphics_context.device),
            wgpu_instance_strong = Arc::strong_count(&self.graphics_context.instance),
            wgpu_ctx_strong = Arc::strong_count(&self.wgpu_ctx),
            "DROP PipelineCtx"
        );
    }
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
