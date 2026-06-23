use sha3::{Digest, Sha3_512};

/// Validates a provided token against the expected one by comparing their
/// SHA3-512 hashes. Returns `true` when they match.
pub(crate) fn validate_token(expected_token: &str, provided_token: &str) -> bool {
    let expected_hash = Sha3_512::digest(expected_token.as_bytes());
    let actual_hash = Sha3_512::digest(provided_token.as_bytes());
    expected_hash == actual_hash
}
