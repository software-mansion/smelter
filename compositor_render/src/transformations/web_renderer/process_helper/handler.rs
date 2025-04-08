use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use compositor_chromium::cef::{self, V8ObjectError};
use log::error;

use crate::error::ErrorStack;
use crate::web_renderer::process_helper::state::FrameInfo;
use crate::web_renderer::{
    EMBED_SOURCE_FRAMES_MESSAGE, GET_FRAME_POSITIONS_MESSAGE, UNEMBED_SOURCE_FRAMES_MESSAGE,
};

use super::state::State;

pub struct RenderProcessHandler {
    state: Arc<Mutex<State>>,
}

impl cef::RenderProcessHandler for RenderProcessHandler {
    fn on_context_created(
        &mut self,
        _browser: &cef::Browser,
        _frame: &cef::Frame,
        context: &cef::V8Context,
    ) {
        context.eval(include_str!("render_frame.js")).unwrap();
    }

    fn on_process_message_received(
        &mut self,
        _browser: &cef::Browser,
        frame: &cef::Frame,
        _source_process: cef::ProcessId,
        message: &cef::ProcessMessage,
    ) -> bool {
        let result = match message.name().as_str() {
            EMBED_SOURCE_FRAMES_MESSAGE => self.embed_sources(message, frame),
            UNEMBED_SOURCE_FRAMES_MESSAGE => self.unembed_source(message),
            GET_FRAME_POSITIONS_MESSAGE => self.send_frame_positions(message, frame),
            name => {
                error!("Error occurred while processing IPC message: Unknown message type: {name}");
                return false;
            }
        };

        if let Err(err) = result {
            error!(
                "Error occurred while processing IPC message: {}",
                ErrorStack::new(err.as_ref()).into_string()
            );
            // Message was not handled
            return false;
        }

        // Message was handled
        true
    }
}

impl RenderProcessHandler {
    pub fn new(state: Arc<Mutex<State>>) -> Self {
        Self { state }
    }

    fn embed_sources(
        &self,
        msg: &cef::ProcessMessage,
        surface: &cef::Frame,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = surface.v8_context()?;
        let ctx_entered = ctx.enter()?;
        let mut global = ctx.global()?;

        const MSG_SIZE: usize = 4;
        for i in (0..msg.size()).step_by(MSG_SIZE) {
            let shmem_path = PathBuf::from(msg.read_string(i)?);
            let id_attribute = msg.read_string(i + 1)?;
            let width = msg.read_int(i + 2)?;
            let height = msg.read_int(i + 3)?;

            if width == 0 && height == 0 {
                continue;
            }

            let frame_info = FrameInfo {
                width: width as u32,
                height: height as u32,
                shmem_path,
                id_attribute,
            };

            self.render_frame(frame_info, &mut global, &ctx_entered)?;
        }

        Ok(())
    }

    fn render_frame(
        &self,
        frame_info: FrameInfo,
        global: &mut cef::V8Global,
        ctx_entered: &cef::V8ContextEntered,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.state.lock().unwrap();
        let source = match state.source(&frame_info.shmem_path) {
            Some(source) => {
                source.ensure_v8_values(&frame_info)?;
                source
            }
            None => state.create_source(frame_info)?,
        };

        let array_buffer = source.create_buffer(ctx_entered);
        global.call_method(
            "smelter_renderFrame",
            &[
                &source.id_attribute_value,
                &array_buffer,
                &source.width,
                &source.height,
            ],
            ctx_entered,
        )?;

        Ok(())
    }

    fn unembed_source(&self, msg: &cef::ProcessMessage) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.state.lock().unwrap();
        let shmem_path = msg.read_string(0)?;
        let shmem_path = PathBuf::from(shmem_path);
        state.remove_source(&shmem_path);

        Ok(())
    }

    fn send_frame_positions(
        &self,
        msg: &cef::ProcessMessage,
        surface: &cef::Frame,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = surface.v8_context()?;
        let ctx_entered = ctx.enter()?;
        let global = ctx.global()?;
        let document = global.document()?;

        let mut response = cef::ProcessMessageBuilder::new(GET_FRAME_POSITIONS_MESSAGE);
        for read_idx in 0..msg.size() {
            let id_attribute = msg.read_string(read_idx)?;
            let element = match document.element_by_id(&id_attribute, &ctx_entered) {
                Ok(element) => element,
                Err(err) => {
                    return Err(MissingElementError { id_attribute, err }.into());
                }
            };

            let rect = element.bounding_rect(&ctx_entered)?;
            response.write_double(rect.x)?;
            response.write_double(rect.y)?;
            response.write_double(rect.width)?;
            response.write_double(rect.height)?;
        }

        surface.send_process_message(cef::ProcessId::Browser, response.build())?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to retrieve element \"{id_attribute}\": {err}")]
pub struct MissingElementError {
    id_attribute: String,
    err: V8ObjectError,
}
