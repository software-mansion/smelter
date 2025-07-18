use std::{fmt::Write, sync::Arc, time::Duration};

use axum::http::HeaderValue;
use rand::{thread_rng, Rng, RngCore};
use tokio::time::sleep;
use tracing::error;

use crate::pipeline::webrtc::error::WhipServerError;

pub(super) fn generate_token() -> Arc<str> {
    let mut bytes = [0u8; 16];
    thread_rng().fill_bytes(&mut bytes);
    let token = bytes.iter().fold(String::new(), |mut acc, byte| {
        if let Err(err) = write!(acc, "{byte:02X}") {
            error!("Cannot generate token: {err:?}")
        }
        acc
    });
    Arc::from(token)
}

pub(super) async fn validate_token(
    expected_token: &str,
    auth_header_value: Option<&HeaderValue>,
) -> Result<(), WhipServerError> {
    match auth_header_value {
        Some(auth_str) => {
            let auth_str = auth_str.to_str().map_err(|_| {
                WhipServerError::Unauthorized("Invalid UTF-8 in header".to_string())
            })?;

            if let Some(token_from_header) = auth_str.strip_prefix("Bearer ") {
                if token_from_header == expected_token {
                    Ok(())
                } else {
                    let nanos = thread_rng().gen_range(0..1000);
                    sleep(Duration::from_nanos(nanos)).await;
                    Err(WhipServerError::Unauthorized(
                        "Invalid or mismatched token provided".to_string(),
                    ))
                }
            } else {
                Err(WhipServerError::Unauthorized(
                    "Authorization header format incorrect".to_string(),
                ))
            }
        }
        None => Err(WhipServerError::Unauthorized(
            "Expected token and authorization header required".to_string(),
        )),
    }
}
