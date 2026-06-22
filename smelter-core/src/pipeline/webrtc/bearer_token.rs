use std::{fmt::Write, sync::Arc};

use axum::http::HeaderValue;
use rand::RngCore;
use sha3::{Digest, Sha3_512};
use tracing::error;

use crate::pipeline::webrtc::error::WhipWhepServerError;

pub(super) fn generate_token() -> Arc<str> {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let token = bytes.iter().fold(String::new(), |mut acc, byte| {
        if let Err(err) = write!(acc, "{byte:02X}") {
            error!("Cannot generate token: {err:?}")
        }
        acc
    });
    Arc::from(token)
}

/// Computes a SHA3-512 hash of the token and returns it as a lowercase hex string.
pub(super) fn hash_token(token: &str) -> Arc<str> {
    let digest = Sha3_512::digest(token.as_bytes());
    let hash = digest.iter().fold(String::new(), |mut acc, byte| {
        if let Err(err) = write!(acc, "{byte:02x}") {
            error!("Cannot hash token: {err:?}")
        }
        acc
    });
    Arc::from(hash)
}

pub(super) async fn validate_token(
    expected_hash: &str,
    auth_header_value: Option<&HeaderValue>,
) -> Result<(), WhipWhepServerError> {
    match auth_header_value {
        Some(auth_str) => {
            let auth_str = auth_str.to_str().map_err(|_| {
                WhipWhepServerError::Unauthorized("Invalid UTF-8 in header".to_string())
            })?;

            if let Some(token_from_header) = auth_str.strip_prefix("Bearer ") {
                let hash_from_header = hash_token(token_from_header);
                if hash_from_header.as_ref() == expected_hash {
                    Ok(())
                } else {
                    Err(WhipWhepServerError::Unauthorized(
                        "Invalid or mismatched token provided".to_string(),
                    ))
                }
            } else {
                Err(WhipWhepServerError::Unauthorized(
                    "Authorization header format incorrect".to_string(),
                ))
            }
        }
        None => Err(WhipWhepServerError::Unauthorized(
            "Unauthorized, \"Authorization\" header is required".to_string(),
        )),
    }
}
