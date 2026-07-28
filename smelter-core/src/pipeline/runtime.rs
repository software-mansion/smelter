use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam_channel::Receiver;
use smelter_render::{Framerate, WgpuCtx};
use tokio::runtime::Runtime;

use crate::{
    event::{Event, EventEmitter},
    graphics_context::GraphicsContext,
    pipeline::{
        PipelineCtx, instance::prepare_side_channel_socket_dir, webrtc::WebrtcSettingEngineCtx,
    },
    queue::QueueContext,
    stats::{StatsMonitor, StatsReport},
};

use crate::prelude::*;

pub struct MediaRuntimeOptions {
    pub sync_point: Instant,
    pub default_buffer_duration: Duration,
    pub side_channel_socket_dir: Option<Arc<Path>>,
    pub output_framerate: Framerate,
    pub mixing_sample_rate: u32,
    pub download_root: Arc<Path>,
    pub graphics_context: GraphicsContext,
    pub wgpu_ctx: Arc<WgpuCtx>,
    pub tokio_rt: Option<Arc<Runtime>>,
    pub webrtc_stun_servers: Arc<Vec<String>>,
    pub webrtc_udp_port_strategy: Option<WebrtcUdpPortStrategy>,
    pub webrtc_nat_1to1_ips: Arc<Vec<String>>,
    pub moq_disable_tls_verification: bool,
}

struct MediaRuntimeLifetime {
    stats_monitor: StatsMonitor,
    webrtc_setting_engine: WebrtcSettingEngineCtx,
}

impl Drop for MediaRuntimeLifetime {
    fn drop(&mut self) {
        self.webrtc_setting_engine.close();
    }
}

#[derive(Clone)]
pub struct MediaRuntime {
    pub(super) ctx: Arc<PipelineCtx>,
    lifetime: Arc<MediaRuntimeLifetime>,
}

impl MediaRuntime {
    pub fn new(options: MediaRuntimeOptions) -> Result<Self, InitPipelineError> {
        if let Some(dir) = options.side_channel_socket_dir.as_deref() {
            prepare_side_channel_socket_dir(dir)?;
        }
        let download_dir: Arc<Path> = options
            .download_root
            .join(format!("smelter-{}", rand::random::<u64>()))
            .into();
        std::fs::create_dir_all(&download_dir).map_err(InitPipelineError::CreateDownloadDir)?;
        let tokio_rt = match options.tokio_rt {
            Some(tokio_rt) => tokio_rt,
            None => Arc::new(Runtime::new().map_err(InitPipelineError::CreateTokioRuntime)?),
        };
        let (stats_monitor, stats_sender) = StatsMonitor::new();
        let webrtc_setting_engine = WebrtcSettingEngineCtx::new(
            options.webrtc_nat_1to1_ips,
            options.webrtc_udp_port_strategy,
            &tokio_rt,
        )?;
        let ctx = Arc::new(PipelineCtx {
            queue_ctx: QueueContext::new(options.sync_point, options.side_channel_socket_dir),
            default_buffer_duration: options.default_buffer_duration,
            mixing_sample_rate: options.mixing_sample_rate,
            output_framerate: options.output_framerate,
            download_dir,
            graphics_context: options.graphics_context,
            wgpu_ctx: options.wgpu_ctx,
            event_emitter: Arc::new(EventEmitter::new()),
            stats_sender,
            webrtc_stun_servers: options.webrtc_stun_servers,
            webrtc_setting_engine: webrtc_setting_engine.clone(),
            moq_disable_tls_verification: options.moq_disable_tls_verification,
            tokio_rt,
            whip_whep_state: None,
            rtmp_state: None,
            moq_state: None,
        });
        Ok(Self {
            ctx,
            lifetime: Arc::new(MediaRuntimeLifetime {
                stats_monitor,
                webrtc_setting_engine,
            }),
        })
    }

    pub fn stats(&self) -> StatsReport {
        self.lifetime.stats_monitor.report()
    }

    pub fn subscribe_events(&self) -> Receiver<Event> {
        self.ctx.event_emitter.subscribe()
    }

    pub fn with_output_framerate(&self, output_framerate: Framerate) -> Self {
        let mut ctx = (*self.ctx).clone();
        ctx.output_framerate = output_framerate;
        Self {
            ctx: Arc::new(ctx),
            lifetime: self.lifetime.clone(),
        }
    }
}
