use core::fmt;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::sync::Arc;

use crate::{transformations::web_renderer::utils, types::Framerate};
use crossbeam_channel::RecvError;
use libcef::cef;
use log::info;

pub struct ChromiumContext {
    instance_id: String,
    framerate: Framerate,
    context: cef::Context,
}

impl ChromiumContext {
    pub fn new(
        framerate: Framerate,
        enable_gpu: bool,
    ) -> Result<Arc<Self>, ChromiumContextInitError> {
        let instance_id = generate_random_id(30);

        info!("Init chromium context");
        let app = ChromiumApp {
            show_fps: false,
            enable_gpu,
        };
        let settings = cef::Settings {
            root_cache_path: utils::get_smelter_instance_tmp_path(&instance_id).join("cef_cache"),
            windowless_rendering_enabled: true,
            log_severity: cef::LogSeverity::Info,
            ..Default::default()
        };

        let context =
            cef::Context::new(app, settings).map_err(ChromiumContextInitError::ContextFailure)?;
        Ok(Arc::new(Self {
            instance_id,
            framerate,
            context,
        }))
    }

    pub(super) fn start_browser(
        &self,
        url: &str,
        state: super::browser_client::BrowserClient,
    ) -> Result<cef::Browser, ChromiumContextInitError> {
        let window_info = cef::WindowInfo {
            windowless_rendering_enabled: true,
        };
        let settings = cef::BrowserSettings {
            windowless_frame_rate: (self.framerate.num as i32) / (self.framerate.den as i32),
            background_color: 0,
        };

        let (tx, rx) = crossbeam_channel::bounded(1);
        let task = cef::Task::new(move || {
            let result = self
                .context
                .start_browser(state, window_info, settings, url);
            tx.send(result).unwrap();
        });

        task.run(cef::ThreadId::UI);
        rx.recv()?.map_err(ChromiumContextInitError::ContextFailure)
    }

    pub fn run_event_loop(&self) -> Result<(), ChromiumEventLoopError> {
        if !self.context.currently_on_thread(cef::ThreadId::UI) {
            return Err(ChromiumEventLoopError::WrongThread);
        }

        self.context.run_message_loop();
        Ok(())
    }

    pub fn run_event_loop_single_iter(&self) -> Result<(), ChromiumEventLoopError> {
        if !self.context.currently_on_thread(cef::ThreadId::UI) {
            return Err(ChromiumEventLoopError::WrongThread);
        }

        self.context.do_message_loop_work();
        Ok(())
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChromiumContextInitError {
    #[error("Chromium context failed: {0}")]
    ContextFailure(#[from] cef::ContextError),

    #[error("Thread communication failed.")]
    ThreadNoResponse(#[from] RecvError),
}

#[derive(Debug, thiserror::Error)]
pub enum ChromiumEventLoopError {
    #[error("Event loop must run on the main thread")]
    WrongThread,
}

struct ChromiumApp {
    show_fps: bool,
    enable_gpu: bool,
}

impl cef::App for ChromiumApp {
    type RenderProcessHandlerType = ();

    fn on_before_command_line_processing(
        &mut self,
        process_type: String,
        command_line: &mut cef::CommandLine,
    ) {
        // Execute only on the main process
        if !process_type.is_empty() {
            return;
        }

        // OSR will not work without this on MacOS
        #[cfg(target_os = "macos")]
        command_line.append_switch("use-mock-keychain");

        if self.show_fps {
            command_line.append_switch("show-fps-counter")
        }
        if !self.enable_gpu {
            command_line.append_switch("disable-gpu");
            // TODO: This is probably only needed in docker container
            command_line.append_switch("disable-software-rasterizer");
        }

        command_line.append_switch("disable-dev-shm-usage");
        command_line.append_switch("disable-gpu-shader-disk-cache");
        command_line.append_switch_with_value("autoplay-policy", "no-user-gesture-required");
    }
}

impl fmt::Debug for ChromiumContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChromiumContext")
            .field("instance_id", &self.instance_id)
            .field("framerate", &self.framerate)
            .finish()
    }
}

pub(crate) fn generate_random_id(length: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect::<String>()
}
