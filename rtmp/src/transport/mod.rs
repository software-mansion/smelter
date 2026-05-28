use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use tls::{TlsClientStream, TlsServerStream};

use crate::{RtmpConnectionError, server::TlsConfig};

pub(crate) mod tls;

/// Transport layer for RTMP connections. Wraps either a plain TCP or a TLS
/// connection and implements [`Read`] + [`Write`].
pub(crate) enum RtmpTransport {
    Tcp(TcpStream),
    TlsClient(Box<TlsClientStream>),
    TlsServer(Box<TlsServerStream>),
}

/// Bounded TCP connect timeout for all client connect attempts. Caps how long
/// the calling thread can block in [`TcpStream::connect_timeout`] when the
/// target is unreachable or blackholed, so background reconnect workers honor
/// cancellation within a bounded window and initial connects fail fast.
const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

impl RtmpTransport {
    pub fn tcp_client(host: &str, port: u16) -> Result<Self, RtmpConnectionError> {
        let socket = connect_tcp(host, port, CLIENT_CONNECT_TIMEOUT)?;
        Self::configure_client_socket(&socket);

        Ok(Self::Tcp(socket))
    }

    pub fn tls_client(host: &str, port: u16) -> Result<Self, RtmpConnectionError> {
        let socket = connect_tcp(host, port, CLIENT_CONNECT_TIMEOUT)?;
        Self::configure_client_socket(&socket);

        let socket = TlsClientStream::new(socket, host)?;
        Ok(Self::TlsClient(Box::new(socket)))
    }

    pub fn tcp_server_stream(socket: TcpStream) -> Self {
        Self::configure_server_socket(&socket);
        Self::Tcp(socket)
    }

    pub fn tls_server_stream(
        socket: TcpStream,
        tls_config: &TlsConfig,
    ) -> Result<Self, RtmpConnectionError> {
        Self::configure_server_socket(&socket);

        let tls_stream = TlsServerStream::new(socket, tls_config)?;
        Ok(Self::TlsServer(Box::new(tls_stream)))
    }

    fn configure_server_socket(socket: &TcpStream) {
        // Socket is intentionally kept in blocking mode with short read/write
        // timeouts to approximate non-blocking behavior and allow cooperative
        // polling using the should_close flag. On some platforms, a socket
        // created during connection inherits options from the listener socket,
        // so we explicitly force it to blocking mode here.
        socket
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp input stream");
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set read timeout");
        socket
            .set_write_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set write timeout");
    }

    fn configure_client_socket(socket: &TcpStream) {
        socket
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp input stream");
        socket
            .set_read_timeout(Some(Duration::from_micros(500)))
            .expect("Cannot set read timeout");
        socket
            .set_write_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set write timeout");
    }
}

fn connect_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, RtmpConnectionError> {
    let mut last_err: Option<io::Error> = None;
    let mut any_addr = false;
    for addr in (host, port).to_socket_addrs()? {
        any_addr = true;
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(socket) => return Ok(socket),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err
        .unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                if any_addr {
                    "no address could be reached"
                } else {
                    "no addresses resolved"
                },
            )
        })
        .into())
}

impl Read for RtmpTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            RtmpTransport::Tcp(tcp) => tcp.read(buf),
            RtmpTransport::TlsClient(s) => s.read(buf),
            RtmpTransport::TlsServer(s) => s.read(buf),
        }
    }
}

impl Write for RtmpTransport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            RtmpTransport::Tcp(tcp) => tcp.write(buf),
            RtmpTransport::TlsClient(s) => s.write(buf),
            RtmpTransport::TlsServer(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            RtmpTransport::Tcp(tcp) => tcp.flush(),
            RtmpTransport::TlsClient(s) => s.flush(),
            RtmpTransport::TlsServer(s) => s.flush(),
        }
    }
}
