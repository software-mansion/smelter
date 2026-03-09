use std::{
    io::{self, Read, Write},
    net::{IpAddr, TcpStream},
    sync::Arc,
};

use rustls::{
    ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
};
use rustls_native_certs::load_native_certs;
use tracing::warn;

use crate::{RtmpConnectionError, server::TlsConfig};

pub(crate) struct TlsClientStream(StreamOwned<ClientConnection, TcpStream>);

impl TlsClientStream {
    pub fn new(socket: TcpStream, host: &str) -> Result<Self, RtmpConnectionError> {
        let server_name = if let Ok(ip) = host.parse::<IpAddr>() {
            ServerName::IpAddress(ip.into())
        } else {
            ServerName::try_from(host.to_owned())?
        };

        let certs = load_native_certs();
        if !certs.errors.is_empty() {
            warn!("Some CA certificates failed to load: {:?}", certs.errors);
        }

        let mut root_store = RootCertStore::empty();
        let (added, skipped) = root_store.add_parsable_certificates(certs.certs);
        if skipped > 0 {
            warn!(%added, %skipped, "Some native CA certificates were rejected by rustls");
        }

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let conn = ClientConnection::new(Arc::new(config), server_name)?;
        Ok(Self(StreamOwned::new(conn, socket)))
    }
}

impl Read for TlsClientStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for TlsClientStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub(crate) struct TlsServerStream(StreamOwned<ServerConnection, TcpStream>);

impl TlsServerStream {
    pub fn new(socket: TcpStream, tls: &TlsConfig) -> Result<Self, RtmpConnectionError> {
        let certs = CertificateDer::pem_file_iter(tls.cert_file.as_ref())
            .map_err(|e| RtmpConnectionError::TlsConfig(format!("Failed to read cert file: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                RtmpConnectionError::TlsConfig(format!("Failed to parse cert file: {e}"))
            })?;

        let key = PrivateKeyDer::from_pem_file(tls.key_file.as_ref())
            .map_err(|e| RtmpConnectionError::TlsConfig(format!("Failed to read key file: {e}")))?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;

        let conn = ServerConnection::new(Arc::new(config))?;
        Ok(Self(StreamOwned::new(conn, socket)))
    }
}

impl Read for TlsServerStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for TlsServerStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
