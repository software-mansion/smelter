use std::{
    io::{self, Read, Write},
    net::{IpAddr, TcpStream},
    sync::{Arc, Mutex},
};

use rustls::{
    ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::ServerName,
};
use rustls_native_certs::load_native_certs;
use tracing::warn;

use crate::RtmpConnectionError;

pub(crate) enum TlsStream {
    Client(Mutex<StreamOwned<ClientConnection, TcpStream>>),
    #[allow(unused)]
    Server(Mutex<StreamOwned<ServerConnection, TcpStream>>),
}

impl TlsStream {
    pub(crate) fn connect(socket: TcpStream, host: &str) -> Result<Self, RtmpConnectionError> {
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

        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(root_store)
        .with_no_client_auth();

        let conn = ClientConnection::new(Arc::new(config), server_name)?;
        Ok(Self::Client(Mutex::new(StreamOwned::new(conn, socket))))
    }

    #[allow(unused)]
    pub(crate) fn accept(
        socket: TcpStream,
        config: Arc<ServerConfig>,
    ) -> Result<Self, RtmpConnectionError> {
        // TODO: support TLS on input
        unimplemented!()
    }

    pub(crate) fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            TlsStream::Client(s) => s.lock().unwrap().read(buf),
            TlsStream::Server(s) => s.lock().unwrap().read(buf),
        }
    }

    pub(crate) fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self {
            TlsStream::Client(s) => s.lock().unwrap().write(buf),
            TlsStream::Server(s) => s.lock().unwrap().write(buf),
        }
    }

    pub(crate) fn flush(&self) -> io::Result<()> {
        match self {
            TlsStream::Client(s) => s.lock().unwrap().flush(),
            TlsStream::Server(s) => s.lock().unwrap().flush(),
        }
    }
}
