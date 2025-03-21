use std::sync::Arc;

use crate::{
    event_loop::{EventLoop, EventLoopRunError},
    transformations::web_renderer::utils,
    types::Framerate,
    utils::random_string,
};
#[cfg(feature = "web_renderer")]
use compositor_chromium::cef;
use crossbeam_channel::RecvError;
use log::info;

use super::WebRendererInitOptions;

pub struct ChromiumContext {
    instance_id: String,
    #[cfg(feature = "web_renderer")]
    pub(super) context: Option<Arc<cef::Context>>,
    framerate: Framerate,
}

impl ChromiumContext {
    pub(crate) fn new(
        opts: WebRendererInitOptions,
        framerate: Framerate,
    ) -> Result<Self, WebRendererContextError> {
        let instance_id = random_string(30);
        #[cfg(not(feature = "web_renderer"))]
        {
            if opts.enable {
                return Err(WebRendererContextError::WebRenderingNotAvailable);
            }
            return Ok(Self {
                instance_id,
                framerate,
            });
        }

        #[cfg(feature = "web_renderer")]
        {
            if !opts.enable {
                info!("Chromium context disabled");
                return Ok(Self {
                    instance_id,
                    framerate,
                    context: None,
                });
            }

            info!("Init chromium context");

            let app = ChromiumApp {
                show_fps: false,
                enable_gpu: opts.enable_gpu,
            };
            let settings = cef::Settings {
                root_cache_path: utils::get_smelter_instance_tmp_path(&instance_id)
                    .join("cef_cache"),
                windowless_rendering_enabled: true,
                log_severity: cef::LogSeverity::Info,
                ..Default::default()
            };

            let context = Arc::new(
                cef::Context::new(app, settings)
                    .map_err(WebRendererContextError::ContextFailure)?,
            );
            Ok(Self {
                instance_id,
                framerate,
                context: Some(context),
            })
        }
    }

    #[cfg(feature = "web_renderer")]
    pub(super) fn start_browser(
        &self,
        url: &str,
        state: super::browser_client::BrowserClient,
    ) -> Result<cef::Browser, WebRendererContextError> {
        let context = self
            .context
            .as_ref()
            .ok_or(WebRendererContextError::NoContext)?;

        let window_info = cef::WindowInfo {
            windowless_rendering_enabled: true,
        };
        let settings = cef::BrowserSettings {
            windowless_frame_rate: (self.framerate.num as i32) / (self.framerate.den as i32),
            background_color: 0,
        };

        let (tx, rx) = crossbeam_channel::bounded(1);
        let task = cef::Task::new(move || {
            let result = context.start_browser(state, window_info, settings, url);
            tx.send(result).unwrap();
        });

        task.run(cef::ThreadId::UI);
        rx.recv()?.map_err(WebRendererContextError::ContextFailure)
    }

    pub fn event_loop(&self) -> Arc<dyn EventLoop> {
        #[cfg(feature = "web_renderer")]
        return self
            .context
            .clone()
            .map(|ctx| ctx as Arc<dyn EventLoop>)
            .unwrap_or_else(|| Arc::new(FallbackEventLoop));
        #[cfg(not(feature = "web_renderer"))]
        return Arc::new(FallbackEventLoop);
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

struct ChromiumApp {
    show_fps: bool,
    enable_gpu: bool,
}

#[cfg(feature = "web_renderer")]
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

#[cfg(feature = "web_renderer")]
impl EventLoop for cef::Context {
    fn run_with_fallback(&self, _fallback: &dyn Fn()) -> Result<(), EventLoopRunError> {
        if !self.currently_on_thread(cef::ThreadId::UI) {
            return Err(EventLoopRunError::WrongThread);
        }

        self.run_message_loop();
        Ok(())
    }
}

struct FallbackEventLoop;

impl EventLoop for FallbackEventLoop {
    fn run_with_fallback(&self, fallback: &dyn Fn()) -> Result<(), EventLoopRunError> {
        fallback();
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WebRendererContextError {
    #[cfg(feature = "web_renderer")]
    #[error("Chromium context failed: {0}")]
    ContextFailure(cef::ContextError),

    #[error("Thread communication failed.")]
    ThreadNoResponse(#[from] RecvError),

    #[error("Chromium context not initialized.")]
    NoContext,

    #[error("Chromium message loop can only run on the main thread.")]
    WrongThreadForMessageLoop,

    #[error("Web rendering feature is not available")]
    WebRenderingNotAvailable,
}
