use std::sync::{Arc, Mutex};

use crate::Resolution;
use bytes::Bytes;
use tracing::error;

use crate::transformations::web_renderer::{FrameData, SourceTransforms};

use super::{
    GET_FRAME_POSITIONS_MESSAGE,
    transformation_matrices::{Position, vertices_transformation_matrix},
};

#[derive(Clone)]
pub(super) struct BrowserClient {
    frame_data: FrameData,
    source_transforms: SourceTransforms,
    resolution: Resolution,
}

impl libcef::Client for BrowserClient {
    type RenderHandlerType = RenderHandler;

    fn render_handler(&self) -> Option<Self::RenderHandlerType> {
        Some(RenderHandler::new(self.frame_data.clone(), self.resolution))
    }

    fn on_process_message_received(
        &mut self,
        _browser: &libcef::Browser,
        _frame: &libcef::Frame,
        _source_process: libcef::ProcessId,
        message: &libcef::ProcessMessage,
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
        msg: &libcef::ProcessMessage,
        index: usize,
    ) -> Result<Position, libcef::ProcessMessageError> {
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

impl libcef::RenderHandler for RenderHandler {
    fn resolution(&self, _browser: &libcef::Browser) -> libcef::Resolution {
        libcef::Resolution {
            width: self.resolution.width,
            height: self.resolution.height,
        }
    }

    fn on_paint(&self, _browser: &libcef::Browser, buffer: &[u8], _resolution: libcef::Resolution) {
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
