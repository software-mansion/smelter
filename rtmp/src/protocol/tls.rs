use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
};

use rustls::{ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName};
use rustls_native_certs::load_native_certs;
use tracing::warn;

use crate::RtmpConnectionError;

pub(crate) struct TlsStream(Mutex<StreamOwned<ClientConnection, TcpStream>>);

impl TlsStream {
    pub(crate) fn new(
        socket: TcpStream,
        server_name: ServerName<'static>,
    ) -> Result<Self, RtmpConnectionError> {
        let certs = load_native_certs();
        if !certs.errors.is_empty() {
            warn!("Some CA certificates failed to load: {:?}", certs.errors);
        }

        let mut root_store = RootCertStore::empty();
        let (added, skipped) = root_store.add_parsable_certificates(certs.certs);
        if skipped > 0 {
            warn!(%added, %skipped, "Some native CA certificates were rejected by rustls");
        }

        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(root_store)
        .with_no_client_auth();

        let conn = ClientConnection::new(Arc::new(config), server_name)?;
        Ok(Self(Mutex::new(StreamOwned::new(conn, socket))))
    }

    pub(crate) fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.lock().unwrap().read(buf)
    }

    pub(crate) fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    pub(crate) fn flush(&self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}
