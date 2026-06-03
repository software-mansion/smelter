//! Self-signed TLS certificate fallback for the MoQ server.
//!
//! When the user does not configure a TLS cert/key, we generate a **self-signed**
//! certificate so the MoQ server can still start for dev/test usage. This is
//! INSECURE and must never be used in production.
//!
//! The cert/key are persisted to `~/.smelter/` so that the SHA-256 fingerprint a
//! human copies out of the logs (to wire into a client via `serverCertificateHashes`)
//! stays stable across restarts. WebTransport caps self-signed cert validity at 14
//! days, so the cert is regenerated once it expires (or if it is missing/corrupted).

use std::fs;
use std::path::Path;

use moq_native::ServerTlsConfig;
use moq_native::rustls::pki_types::pem::PemObject;
use moq_native::rustls::pki_types::{CertificateDer, UnixTime};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use tracing::warn;
use webpki::{EndEntityCert, KeyUsage, anchor_from_trusted_cert};

const CERT_FILE_NAME: &str = "moq_cert.pem";
const KEY_FILE_NAME: &str = "moq_key.pem";

#[derive(Debug, thiserror::Error)]
pub enum SelfSignedTlsError {
    #[error("Could not determine the home directory to store the MoQ certificate.")]
    NoHomeDir,

    #[error("Filesystem error while handling the MoQ certificate.")]
    Io(#[from] std::io::Error),

    #[error("Failed to generate a self-signed MoQ certificate.")]
    Rcgen(#[from] rcgen::Error),
}

/// Load a previously persisted self-signed certificate from `~/.smelter`, or
/// generate (and persist) a new one if it is missing, expired, or corrupted.
///
/// Returns a [`ServerTlsConfig`] pointing at the cert/key files on disk; moq loads
/// the files itself at `Server::init`.
pub fn load_or_create_self_signed_tls() -> Result<ServerTlsConfig, SelfSignedTlsError> {
    let dir = dirs::home_dir()
        .ok_or(SelfSignedTlsError::NoHomeDir)?
        .join(".smelter");
    fs::create_dir_all(&dir)?;

    let cert_path = dir.join(CERT_FILE_NAME);
    let key_path = dir.join(KEY_FILE_NAME);

    let cert_der = if cert_path.exists() && key_path.exists() {
        match read_and_validate(&cert_path) {
            Ok(cert_der) => cert_der,
            Err(error) => {
                warn!(
                    %error,
                    "Existing self-signed MoQ certificate is invalid (expired or corrupted). Regenerating."
                );
                generate(&cert_path, &key_path)?
            }
        }
    } else {
        generate(&cert_path, &key_path)?
    };

    let fingerprint = fingerprint(&cert_der);
    warn!(
        "Using INSECURE self-signed MoQ TLS certificate. Generated/loaded from ~/.smelter. \
         NEVER use in production. Cert SHA-256 (for client serverCertificateHashes): {fingerprint}"
    );

    let mut tls = ServerTlsConfig::default();
    tls.cert = vec![cert_path];
    tls.key = vec![key_path];
    Ok(tls)
}

/// Read the on-disk cert and verify it is currently valid (self-signed, server auth).
/// Returns the certificate DER bytes on success. Any failure (parse error, expiry,
/// corruption) is returned as an error so the caller can regenerate.
fn read_and_validate(cert_path: &Path) -> Result<Vec<u8>, anyhow::Error> {
    let cert_der = CertificateDer::from_pem_file(cert_path)?;

    // A self-signed certificate acts as its own trust anchor.
    let anchor = anchor_from_trusted_cert(&cert_der)?;
    let ee = EndEntityCert::try_from(&cert_der)?;

    let provider = moq_native::rustls::crypto::aws_lc_rs::default_provider();
    let sig_algs = provider.signature_verification_algorithms.all;

    ee.verify_for_usage(
        sig_algs,
        &[anchor],
        &[],
        UnixTime::now(),
        KeyUsage::server_auth(),
        None,
        None,
    )?;

    Ok(cert_der.as_ref().to_vec())
}

/// Generate a fresh self-signed certificate and persist it to disk.
///
/// Mirrors moq-native's own generator: SANs for `localhost`/`127.0.0.1`/`::1` and a
/// 14-day validity window starting yesterday (WebTransport caps self-signed certs at
/// two weeks, and the back-dated start tolerates clock drift).
fn generate(cert_path: &Path, key_path: &Path) -> Result<Vec<u8>, SelfSignedTlsError> {
    let key_pair = rcgen::KeyPair::generate()?;

    let mut params = rcgen::CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])?;
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = params.not_before + Duration::days(14);

    let cert = params.self_signed(&key_pair)?;

    fs::write(cert_path, cert.pem())?;
    write_key(key_path, &key_pair.serialize_pem())?;

    Ok(cert.der().to_vec())
}

/// Write the private key, restricting it to owner read/write (`0600`).
/// The file is created with the restrictive mode directly to avoid a window in
/// which a world-readable key exists on disk.
fn write_key(key_path: &Path, pem: &str) -> Result<(), std::io::Error> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(key_path)?;
    file.write_all(pem.as_bytes())?;
    Ok(())
}

/// SHA-256 over the certificate DER, hex-encoded. Matches moq's own fingerprint scheme.
fn fingerprint(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    data_encoding::HEXLOWER.encode(&digest)
}
