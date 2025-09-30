use std::sync::{Arc, Mutex};

use log::info;

use super::{handler::RenderProcessHandler, state::State};

pub struct App {
    // `Arc` is used instead of `Rc` because we are not sure what CEF does underneath
    state: Arc<Mutex<State>>,
}

impl libcef::App for App {
    type RenderProcessHandlerType = RenderProcessHandler;

    fn on_before_command_line_processing(
        &mut self,
        process_type: String,
        _command_line: &mut libcef::CommandLine,
    ) {
        info!("Chromium {process_type} subprocess started");
    }

    fn render_process_handler(&self) -> Option<Self::RenderProcessHandlerType> {
        Some(RenderProcessHandler::new(self.state.clone()))
    }
}

impl App {
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new())),
        }
    }
}
