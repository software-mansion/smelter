use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;

#[derive(Debug)]
pub enum WhipWhepServerError {
    BadRequest(String),
    InternalError(String),
    Unauthorized(String),
    NotFound(String),
}

impl<T> From<T> for WhipWhepServerError
where
    T: std::error::Error + 'static,
{
    fn from(err: T) -> Self {
        WhipWhepServerError::InternalError(err.to_string())
    }
}

impl std::fmt::Display for WhipWhepServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WhipWhepServerError::InternalError(message) => f.write_str(message),
            WhipWhepServerError::BadRequest(message) => f.write_str(message),
            WhipWhepServerError::Unauthorized(message) => f.write_str(message),
            WhipWhepServerError::NotFound(message) => f.write_str(message),
        }
    }
}

impl IntoResponse for WhipWhepServerError {
    fn into_response(self) -> Response {
        match self {
            WhipWhepServerError::InternalError(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
            }
            WhipWhepServerError::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, message).into_response()
            }
            WhipWhepServerError::Unauthorized(message) => {
                (StatusCode::UNAUTHORIZED, message).into_response()
            }
            WhipWhepServerError::NotFound(message) => {
                (StatusCode::NOT_FOUND, message).into_response()
            }
        }
    }
}
