use std::{fmt::Write, time::Duration};

use axum::http::HeaderValue;
use rand::{rngs::StdRng, thread_rng, Rng, RngCore, SeedableRng};
use tokio::time::sleep;
use tracing::error;

use crate::pipeline::webrtc::error::WhipServerError;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    thread_rng().fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut acc, byte| {
        if let Err(err) = write!(acc, "{byte:02X}") {
            error!("Cannot generate token: {err:?}")
        }
        acc
    })
}

pub async fn validate_token(
    expected_token: Option<String>,
    auth_header_value: Option<&HeaderValue>,
) -> Result<(), WhipServerError> {
    match (expected_token, auth_header_value) {
        (Some(bearer_token), Some(auth_str)) => {
            let auth_str = auth_str.to_str().map_err(|_| {
                WhipServerError::Unauthorized("Invalid UTF-8 in header".to_string())
            })?;

            if let Some(token_from_header) = auth_str.strip_prefix("Bearer ") {
                if token_from_header == bearer_token {
                    Ok(())
                } else {
                    let mut rng = StdRng::from_entropy();
                    let nanos = rng.gen_range(0..1000);
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
        _ => Err(WhipServerError::Unauthorized(
            "Expected token and authorization header required".to_string(),
        )),
    }
}
