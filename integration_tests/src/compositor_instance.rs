use anyhow::{anyhow, Result};
use crossbeam_channel::Sender;
use reqwest::StatusCode;
use smelter::{
    config::{read_config, Config},
    logger::{self},
    server::run_api,
    state::ApiState,
};
use std::{
    env,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, OnceLock,
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
}

impl Drop for CompositorInstance {
    fn drop(&mut self) {
        self.should_close_sender.send(()).unwrap();
    }
}

impl CompositorInstance {
    /// api port and  start_whip_whep  are overwritten in config
    pub fn start(config: Option<Config>) -> Self {
        init_compositor_prerequisites();
        let api_port = get_free_port();
        let mut config = config.unwrap_or(read_config());
        config.api_port = api_port;
        config.whip_whep_enable = false;

        info!("Starting Smelter Integration Test with config:\n{config:#?}",);

        let (should_close_sender, should_close_receiver) = crossbeam_channel::bounded(1);
        let runtime = Arc::new(Runtime::new().unwrap());
        let (state, _event_loop) = ApiState::new(config, runtime.clone()).unwrap();

        thread::Builder::new()
            .name("HTTP server startup thread".to_string())
            .spawn(move || {
                run_api(state, runtime.clone(), should_close_receiver).unwrap();
            })
            .unwrap();

        let instance = CompositorInstance {
            api_port,
            http_client: reqwest::blocking::Client::new(),
            should_close_sender,
        };
        instance.wait_for_start(Duration::from_secs(30)).unwrap();
        instance
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

    pub fn get_port(&self) -> u16 {
        get_free_port()
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
}

fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port = LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 10_000)));
    port.fetch_add(1, Ordering::Relaxed)
}

fn init_compositor_prerequisites() {
    static GLOBAL_PREREQUISITES_INITIALIZED: OnceLock<()> = OnceLock::new();
    GLOBAL_PREREQUISITES_INITIALIZED.get_or_init(|| {
        env::set_var("SMELTER_WEB_RENDERER_ENABLE", "0");
        ffmpeg_next::format::network::init();
        let config = read_config();
        logger::init_logger(config.logger);
    });
}
