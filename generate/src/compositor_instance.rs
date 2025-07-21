use anyhow::{anyhow, Result};
use compositor_pipeline::{
    event::Event,
    graphics_context::{GraphicsContext, GraphicsContextOptions},
    Pipeline, PipelineOptions,
};
use compositor_render::WgpuFeatures;
use crossbeam_channel::{Receiver, Sender};
use reqwest::StatusCode;
use smelter::{config::read_config, logger, server::run_api, state::ApiState};
use std::{
    env,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tracing::info;

pub struct CompositorInstance {
    pub api_port: u16,
    pub http_client: reqwest::blocking::Client,
    pub should_close_sender: Sender<()>,
    pub events: Receiver<compositor_pipeline::event::Event>,
}

impl Drop for CompositorInstance {
    fn drop(&mut self) {
        self.should_close_sender.send(()).unwrap();
    }
}

impl CompositorInstance {
    pub fn start() -> Self {
        init_compositor_prerequisites();
        let mut config = read_config();
        let mut options = PipelineOptions::from(&config);
        let api_port = get_free_port();
        config.api_port = api_port;
        options.queue_options.ahead_of_time_processing = true;
        options.queue_options.never_drop_output_frames = true;
        options.start_whip_whep = false;
        options.wgpu_ctx = Some(graphics_context());
        options.tokio_rt = Some(runtime());

        info!("Starting Smelter Integration Test with config:\n{config:#?}",);

        let (should_close_sender, should_close_receiver) = crossbeam_channel::bounded(1);
        let (pipeline, _) = Pipeline::new(options).unwrap();
        let state = ApiState {
            pipeline: Arc::new(Mutex::new(pipeline)),
            config,
        };

        let events = state.pipeline.lock().unwrap().subscribe_pipeline_events();
        thread::Builder::new()
            .name("HTTP server startup thread".to_string())
            .spawn(move || {
                run_api(state, runtime(), should_close_receiver).unwrap();
            })
            .unwrap();

        let instance = CompositorInstance {
            events,
            api_port,
            http_client: reqwest::blocking::Client::new(),
            should_close_sender,
        };
        instance.wait_for_start(Duration::from_secs(30)).unwrap();
        instance
    }

    pub fn get_port(&self) -> u16 {
        get_free_port()
    }

    pub fn send_request(&self, path: &str, request_body: serde_json::Value) -> Result<()> {
        let resp = self
            .http_client
            .post(format!("http://127.0.0.1:{}/api/{}", self.api_port, path))
            .timeout(Duration::from_secs(100))
            .json(&request_body)
            .send()?;

        if resp.status() >= StatusCode::BAD_REQUEST {
            let status = resp.status();
            let request_str = serde_json::to_string_pretty(&request_body).unwrap();
            let body_str = resp.text().unwrap();
            return Err(anyhow::anyhow!(
                "Request failed with status: {status}\nRequest: {request_str}\nResponse: {body_str}",
            ));
        }

        Ok(())
    }

    fn wait_for_start(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        loop {
            let response = self
                .http_client
                .get(format!("http://127.0.0.1:{}/status", self.api_port))
                .timeout(Duration::from_secs(1))
                .send();
            if response.is_ok() {
                return Ok(());
            }
            if start + timeout < Instant::now() {
                return Err(anyhow!("Failed to connect to instance."));
            }
        }
    }

    pub fn wait_for_output_end(&self) {
        loop {
            if let Event::OutputDone(_) = self.events.recv().unwrap() {
                return;
            }
        }
    }
}

fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port = LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 50_000)));
    port.fetch_add(1, Ordering::Relaxed)
}

fn init_compositor_prerequisites() {
    static GLOBAL_PREREQUISITES_INITIALIZED: OnceLock<()> = OnceLock::new();
    GLOBAL_PREREQUISITES_INITIALIZED.get_or_init(|| {
        env::set_var("SMELTER_WEB_RENDERER_ENABLE", "0");
        ffmpeg_next::format::network::init();
        logger::init_logger(read_config().logger);
    });
}

fn graphics_context() -> GraphicsContext {
    static CTX: OnceLock<GraphicsContext> = OnceLock::new();
    CTX.get_or_init(|| {
        GraphicsContext::new(GraphicsContextOptions {
            features: WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
            ..Default::default()
        })
        .unwrap()
    })
    .clone()
}

fn runtime() -> Arc<Runtime> {
    static CTX: OnceLock<Arc<Runtime>> = OnceLock::new();
    CTX.get_or_init(|| Arc::new(Runtime::new().unwrap()))
        .clone()
}
