use std::sync::{Arc, Mutex};

use crate::Resolution;
use bytes::Bytes;
use compositor_chromium::cef;
use log::error;

use crate::transformations::web_renderer::{FrameData, SourceTransforms};

use super::{
    transformation_matrices::{vertices_transformation_matrix, Position},
    GET_FRAME_POSITIONS_MESSAGE,
};

#[derive(Clone)]
pub(super) struct BrowserClient {
    frame_data: FrameData,
    source_transforms: SourceTransforms,
    resolution: Resolution,
}

impl cef::Client for BrowserClient {
    type RenderHandlerType = RenderHandler;

    fn render_handler(&self) -> Option<Self::RenderHandlerType> {
        Some(RenderHandler::new(self.frame_data.clone(), self.resolution))
    }

    fn on_process_message_received(
        &mut self,
        _browser: &cef::Browser,
        _frame: &cef::Frame,
        _source_process: cef::ProcessId,
        message: &cef::ProcessMessage,
    ) -> bool {
        match message.name().as_str() {
            GET_FRAME_POSITIONS_MESSAGE => {
                let mut transforms_matrices = Vec::new();
                for i in (0..message.size()).step_by(4) {
                    let position = match Self::read_frame_position(message, i) {
                        Ok(position) => position,
                        Err(err) => {
                            error!(
                                "Error occurred while reading frame positions from IPC message: {err}"
                            );
                            return true;
                        }
                    };

                    let transformations_matrix =
                        vertices_transformation_matrix(&position, &self.resolution);

                    transforms_matrices.push(transformations_matrix);
                }

                let mut source_transforms = self.source_transforms.lock().unwrap();
                *source_transforms = transforms_matrices;
            }
            ty => error!("Unknown process message type \"{ty}\""),
        }
        true
    }
}

impl BrowserClient {
    pub fn new(
        frame_data: FrameData,
        source_transforms: SourceTransforms,
        resolution: Resolution,
    ) -> Self {
        Self {
            frame_data,
            source_transforms,
            resolution,
        }
    }

    fn read_frame_position(
        msg: &cef::ProcessMessage,
        index: usize,
    ) -> Result<Position, cef::ProcessMessageError> {
        let x = msg.read_double(index)?;
        let y = msg.read_double(index + 1)?;
        let width = msg.read_double(index + 2)?;
        let height = msg.read_double(index + 3)?;

        Ok(Position {
            top: y as f32,
            left: x as f32,
            width: width as f32,
            height: height as f32,
            rotation_degrees: 0.0,
        })
    }
}

pub(super) struct RenderHandler {
    frame_data: FrameData,
    resolution: Resolution,
}

impl cef::RenderHandler for RenderHandler {
    fn resolution(&self, _browser: &cef::Browser) -> cef::Resolution {
        cef::Resolution {
            width: self.resolution.width,
            height: self.resolution.height,
        }
    }

    fn on_paint(&self, _browser: &cef::Browser, buffer: &[u8], _resolution: cef::Resolution) {
        let mut frame_data = self.frame_data.lock().unwrap();
        *frame_data = Bytes::copy_from_slice(buffer);
    }
}

impl RenderHandler {
    pub fn new(frame_data: Arc<Mutex<Bytes>>, resolution: Resolution) -> Self {
        Self {
            frame_data,
            resolution,
        }
    }
}
