use std::fmt::Write;

use sha3::{Digest, Sha3_512};
use tracing::warn;

/// Computes a SHA3-512 hash of the token and returns it as a lowercase hex string.
fn hash_token(token: &str) -> String {
    let digest = Sha3_512::digest(token.as_bytes());
    digest.iter().fold(String::new(), |mut acc, byte| {
        if let Err(err) = write!(acc, "{byte:02x}") {
            warn!("Cannot hash token: {err:?}");
        }
        acc
    })
}

/// Validates a provided token against the expected one by comparing their
/// SHA3-512 hashes. Returns `true` when they match.
pub(crate) fn validate_token(expected_token: &str, provided_token: &str) -> bool {
    hash_token(provided_token) == hash_token(expected_token)
}
