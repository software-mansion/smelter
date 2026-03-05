use std::{
    io::{self, Read, Write},
    net::{IpAddr, TcpStream},
    sync::Arc,
};

use rustls::{ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName};
use rustls_native_certs::load_native_certs;
use tracing::warn;

use crate::RtmpConnectionError;

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
