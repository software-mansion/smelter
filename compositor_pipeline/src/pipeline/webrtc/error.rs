use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;

#[derive(Debug)]
pub enum WhipServerError {
    BadRequest(String),
    InternalError(String),
    Unauthorized(String),
    CodecNegotiationError(String),
    NotFound(String),
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
        f.write_str(match self {
            WhipServerError::InternalError(message) => message,
            WhipServerError::BadRequest(message) => message,
            WhipServerError::Unauthorized(message) => message,
            WhipServerError::CodecNegotiationError(message) => message,
            WhipServerError::NotFound(message) => message,
        })
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
        }
    }
}
