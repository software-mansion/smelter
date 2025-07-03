use axum::response::{IntoResponse, Response};
use compositor_render::error::ErrorStack;
use reqwest::StatusCode;

use crate::error::DecoderInitError;

#[derive(Debug)]
pub enum WhipServerError {
    BadRequest(String),
    InternalError(String),
    Unauthorized(String),
    CodecNegotiationError(String),
    NotFound(String),
    DecoderInitError(DecoderInitError),
}

impl<T> From<T> for WhipServerError
where
    T: std::error::Error + 'static,
{
    fn from(err: T) -> Self {
        WhipServerError::InternalError(err.to_string())
    }
}

impl std::fmt::Display for WhipServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WhipServerError::InternalError(message) => f.write_str(message),
            WhipServerError::BadRequest(message) => f.write_str(message),
            WhipServerError::Unauthorized(message) => f.write_str(message),
            WhipServerError::CodecNegotiationError(message) => f.write_str(message),
            WhipServerError::NotFound(message) => f.write_str(message),
            WhipServerError::DecoderInitError(err) => {
                f.write_str(&ErrorStack::new(err).into_string())
            }
        }
    }
}

impl IntoResponse for WhipServerError {
    fn into_response(self) -> Response {
        match self {
            WhipServerError::InternalError(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
            }
            WhipServerError::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, message).into_response()
            }
            WhipServerError::Unauthorized(message) => {
                (StatusCode::UNAUTHORIZED, message).into_response()
            }
            WhipServerError::CodecNegotiationError(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
            }
            WhipServerError::NotFound(message) => (StatusCode::NOT_FOUND, message).into_response(),
            WhipServerError::DecoderInitError(err) => {
                (StatusCode::BAD_REQUEST, ErrorStack::new(&err).into_string()).into_response()
            }
        }
    }
}
