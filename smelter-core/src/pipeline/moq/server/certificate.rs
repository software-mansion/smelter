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

use std::{fs, path::Path};

use moq_native::{
    ServerTlsConfig,
    rustls::pki_types::{
        CertificateDer, PrivateKeyDer, UnixTime,
        pem::{Error as PemError, PemObject},
    },
};
use rcgen::KeyPair;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use tracing::warn;
use webpki::{EndEntityCert, KeyUsage, anchor_from_trusted_cert};

type CertificatePem = String;
type KeyPem = String;

const CERT_FILE_NAME: &str = "moq_cert.pem";
const KEY_FILE_NAME: &str = "moq_key.pem";

#[derive(Debug, thiserror::Error)]
pub enum SelfSignedTlsError {
    #[error("Could not determine the home directory to store the MoQ certificate.")]
    NoHomeDir,

    #[error("Filesystem error while handling the MoQ certificate: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to generate a self-signed MoQ certificate: {0}")]
    Rcgen(#[from] rcgen::Error),

    #[error("Failed to convert PEM to DER: {0}")]
    Rustls(#[from] PemError),
}

/// Load a previously persisted auto-generated certificate from `~/.smelter`, or
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
        match read_from_disk(&cert_path, &key_path) {
            Ok((cert_pem, key_pem)) => match validate(&cert_pem, &key_pem) {
                Ok(c) => c,
                Err(_) => generate_and_write(&cert_path, &key_path)?,
            },
            Err(_) => generate_and_write(&cert_path, &key_path)?,
        }
    } else {
        generate_and_write(&cert_path, &key_path)?
    };

    let sha256_fingerprint = fingerprint(&cert_der);
    let location = dir.display();
    warn!(
        sha256_fingerprint,
        %location,
        "Using auto-generated, self-signed MoQ TLS certificate. Make sure to configure proper certs in production."
    );

    let mut tls = ServerTlsConfig::default();
    tls.cert = vec![cert_path];
    tls.key = vec![key_path];
    Ok(tls)
}

/// Validate the on-disk cert. Verify if it is currently valid (self-signed, server auth),
/// and confirm the private key still parses. Returns the certificate DER bytes on
/// success. Any failure (parse error, expiry, corruption) is returned as an error so
/// the caller can regenerate both files.
fn validate(cert_pem: &str, key_pem: &str) -> Result<Vec<u8>, anyhow::Error> {
    // Validates if the key is corrupted
    PrivateKeyDer::from_pem_slice(key_pem.as_bytes())?;

    let cert_der = CertificateDer::from_pem_slice(cert_pem.as_bytes())?;

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

    Ok(cert_der.to_vec())
}

/// Generate a fresh self-signed certificate.
///
/// Mirrors moq-native's own generator: SANs for `localhost`/`127.0.0.1`/`::1` and a
/// 14-day validity window starting yesterday (WebTransport caps self-signed certs at
/// two weeks, and the back-dated start tolerates clock drift).
fn generate() -> Result<(CertificatePem, KeyPem), SelfSignedTlsError> {
    let key_pair = KeyPair::generate()?;

    let mut params = rcgen::CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])?;
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = params.not_before + Duration::days(14);

    let cert_pem = params.self_signed(&key_pair)?.pem();

    Ok((cert_pem, key_pair.serialize_pem()))
}

/// SHA-256 over the certificate DER, hex-encoded. Matches moq's own fingerprint scheme.
fn fingerprint(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    data_encoding::HEXLOWER.encode(&digest)
}

fn generate_and_write(cert_path: &Path, key_path: &Path) -> Result<Vec<u8>, SelfSignedTlsError> {
    let (cert_pem, key_pem) = generate()?;
    let cert_der = CertificateDer::from_pem_slice(cert_pem.as_bytes())?;
    write_to_disk(cert_path, key_path, &cert_pem, &key_pem)?;
    Ok(cert_der.to_vec())
}

fn write_to_disk(
    cert_path: &Path,
    key_path: &Path,
    cert_pem: &CertificatePem,
    key_pem: &KeyPem,
) -> Result<(), std::io::Error> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    fs::write(cert_path, cert_pem)?;

    // Private key is restricted to owner read/write (`0600`).
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(key_path)?;
    file.write_all(key_pem.as_bytes())?;
    Ok(())
}

fn read_from_disk(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(CertificatePem, KeyPem), SelfSignedTlsError> {
    let cert_pem = fs::read_to_string(cert_path)?;
    let key_pem = fs::read_to_string(key_path)?;

    Ok((cert_pem, key_pem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_cert_validates() {
        let (cert_pem, key_pem) = generate().unwrap();

        // A freshly generated, valid self-signed cert must pass validation, otherwise
        // we would regenerate (and change the fingerprint) on every restart.
        let der = validate(&cert_pem, &key_pem).expect("freshly generated cert must validate");

        // Validating the same cert again yields identical DER and a stable fingerprint.
        let reread = validate(&cert_pem, &key_pem).expect("cert must validate again");
        assert_eq!(der, reread, "validation must be deterministic");
        assert_eq!(fingerprint(&der), fingerprint(&reread));
    }

    #[test]
    fn corrupted_key_fails_validation() {
        let (cert_pem, _key_pem) = generate().unwrap();
        validate(&cert_pem, "not a valid key").expect_err("corrupted key must fail validation");
    }
}
