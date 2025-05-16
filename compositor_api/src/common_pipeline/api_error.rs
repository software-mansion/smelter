use std::fmt::Display;

use axum::{http::StatusCode, response::IntoResponse};
use compositor_pipeline::error::{ErrorType, PipelineErrorInfo};
use compositor_render::error::ErrorStack;
use serde::Serialize;

use crate::*;

pub struct ApiError {
    pub error_code: &'static str,
    pub message: String,
    pub stack: Vec<String>,
    pub http_status_code: StatusCode,
}

impl ApiError {
    pub fn new(error_code: &'static str, message: String, http_status_code: StatusCode) -> Self {
        ApiError {
            error_code,
            message: message.clone(),
            stack: vec![message],
            http_status_code,
        }
    }

    pub fn malformed_request(err: &dyn Display) -> Self {
        ApiError::new(
            "MALFORMED_REQUEST",
            format!("Received malformed request:\n{err}"),
            StatusCode::BAD_REQUEST,
        )
    }
}

impl<T> From<T> for ApiError
where
    T: std::error::Error + 'static,
    PipelineErrorInfo: for<'a> From<&'a T>,
{
    fn from(err: T) -> Self {
        let stack: Vec<String> = ErrorStack::new(&err).map(ToString::to_string).collect();
        let err_info = PipelineErrorInfo::from(&err);
        ApiError {
            error_code: err_info.error_code,
            message: stack.first().unwrap().clone(),
            stack,
            http_status_code: match err_info.error_type {
                ErrorType::UserError => StatusCode::BAD_REQUEST,
                ErrorType::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
                ErrorType::EntityNotFound => StatusCode::NOT_FOUND,
            },
        }
    }
}

impl From<TypeError> for ApiError {
    fn from(err: TypeError) -> Self {
        ApiError::malformed_request(&err)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        #[derive(Serialize)]
        struct ErrorResponse {
            error_code: &'static str,
            message: String,
            stack: Vec<String>,
        }

        let body = axum::Json(ErrorResponse {
            error_code: self.error_code,
            message: self.message,
            stack: self.stack,
        });
        (self.http_status_code, body).into_response()
    }
}
