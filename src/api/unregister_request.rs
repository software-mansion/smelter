use std::time::Duration;

use compositor_render::{error::ErrorStack, RegistryType};
use log::error;

use crate::error::ApiError;

use super::{Api, ResponseHandler, UnregisterRequest};

pub fn handle_register_request(
    api: &mut Api,
    request: UnregisterRequest,
) -> Result<ResponseHandler, ApiError> {
    match request {
        UnregisterRequest::InputStream {
            input_id,
            schedule_at_ms,
        } => {
            match schedule_at_ms {
                Some(schedule_at_ms) => {
                    let pipeline = api.pipeline.clone();
                    let schedule_at = Duration::from_secs_f64(schedule_at_ms / 1000.0);
                    api.pipeline().queue().schedule_event(schedule_at, move || {
                        if let Err(err) =
                            pipeline.lock().unwrap().unregister_input(&input_id.into())
                        {
                            error!(
                                "Error while running scheduled input unregister: {}",
                                ErrorStack::new(&err).into_string()
                            )
                        }
                    });
                }
                None => {
                    api.pipeline().unregister_input(&input_id.into())?;
                }
            }
            Ok(ResponseHandler::Ok)
        }
        UnregisterRequest::OutputStream {
            output_id,
            schedule_at_ms,
        } => {
            match schedule_at_ms {
                Some(schedule_at_ms) => {
                    let pipeline = api.pipeline.clone();
                    let schedule_at = Duration::from_secs_f64(schedule_at_ms / 1000.0);
                    api.pipeline().queue().schedule_event(schedule_at, move || {
                        if let Err(err) = pipeline
                            .lock()
                            .unwrap()
                            .unregister_output(&output_id.into())
                        {
                            error!(
                                "Error while running scheduled output unregister: {}",
                                ErrorStack::new(&err).into_string()
                            )
                        }
                    });
                }
                None => {
                    api.pipeline().unregister_output(&output_id.into())?;
                }
            }
            Ok(ResponseHandler::Ok)
        }
        UnregisterRequest::Shader { shader_id } => {
            api.pipeline()
                .unregister_renderer(&shader_id.into(), RegistryType::Shader)?;
            Ok(ResponseHandler::Ok)
        }
        UnregisterRequest::WebRenderer { instance_id } => {
            api.pipeline()
                .unregister_renderer(&instance_id.into(), RegistryType::WebRenderer)?;

            Ok(ResponseHandler::Ok)
        }
        UnregisterRequest::Image { image_id } => {
            api.pipeline()
                .unregister_renderer(&image_id.into(), RegistryType::Image)?;
            Ok(ResponseHandler::Ok)
        }
    }
}
