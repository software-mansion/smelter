use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bytes::Buf;

use crate::{RtmpConnectionError, RtmpStreamError};

use super::tls::TlsStream;

pub(crate) enum Socket {
    Tcp(TcpStream),
    Tls(Box<TlsStream>),
}

impl Socket {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Socket::Tcp(tcp) => (&*tcp).read(buf),
            Socket::Tls(tls) => tls.read(buf),
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Socket::Tcp(tcp) => (&*tcp).write(buf),
            Socket::Tls(tls) => tls.write(buf),
        }
    }

    fn flush(&self) -> io::Result<()> {
        match self {
            Socket::Tcp(tcp) => (&*tcp).flush(),
            Socket::Tls(tls) => tls.flush(),
        }
    }
}

pub(crate) struct NonBlockingSocket {
    inner: Socket,
    should_close: Arc<AtomicBool>,
}

impl NonBlockingSocket {
    pub fn new(
        host: &str,
        port: u16,
        use_tls: bool,
        should_close: Arc<AtomicBool>,
    ) -> Result<Self, RtmpConnectionError> {
        let stream = TcpStream::connect((host, port))?;
        configure_socket(&stream);
        let socket = if use_tls {
            Socket::Tls(Box::new(TlsStream::connect(stream, host)?))
        } else {
            Socket::Tcp(stream)
        };
        Ok(Self {
            inner: socket,
            should_close,
        })
    }

    pub fn from_tcp(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        configure_socket(&socket);
        Self {
            inner: Socket::Tcp(socket),
            should_close,
        }
    }

    pub fn split(self) -> (BufferedReader, BufferedWriter) {
        let socket = Arc::new(self);
        let reader = BufferedReader::new(socket.clone());
        let writer = BufferedWriter::new(socket);
        (reader, writer)
    }

    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(crate) struct BufferedReader {
    socket: Arc<NonBlockingSocket>,
    buf: VecDeque<u8>,
    read_buf: Vec<u8>,
}

impl BufferedReader {
    fn new(socket: Arc<NonBlockingSocket>) -> Self {
        Self {
            socket,
            buf: VecDeque::new(),
            read_buf: vec![0; 65536],
        }
    }

    pub(crate) fn read_until_buffer_size(
        &mut self,
        buf_size: usize,
    ) -> Result<(), RtmpStreamError> {
        loop {
            if self.buf.len() >= buf_size {
                return Ok(());
            }
            match self.socket.read(&mut self.read_buf) {
                Ok(0) => {
                    return Err(
                        io::Error::new(ErrorKind::UnexpectedEof, "connection closed").into(),
                    );
                }
                Ok(read_bytes) => {
                    self.buf.extend(self.read_buf[0..read_bytes].iter());
                }
                Err(err) => {
                    let should_close = self.socket.should_close.load(Ordering::Relaxed);
                    match err.kind() {
                        ErrorKind::WouldBlock | ErrorKind::TimedOut if !should_close => {
                            continue;
                        }
                        _ => {
                            return Err(err.into());
                        }
                    }
                }
            };
        }
    }

    pub(crate) fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), RtmpStreamError> {
        self.read_until_buffer_size(buf.len())?;
        self.buf.copy_to_slice(buf);
        Ok(())
    }

    pub(crate) fn data(&self) -> &VecDeque<u8> {
        &self.buf
    }

    pub(crate) fn data_mut(&mut self) -> &mut VecDeque<u8> {
        &mut self.buf
    }
}

pub(crate) struct BufferedWriter {
    socket: Arc<NonBlockingSocket>,
    buf: Vec<u8>,
}

impl BufferedWriter {
    fn new(socket: Arc<NonBlockingSocket>) -> Self {
        Self {
            socket,
            buf: Vec::new(),
        }
    }
}

impl BufferedWriter {
    fn write_to_socket(&mut self) -> Result<(), io::Error> {
        while !self.buf.is_empty() {
            match self.socket.write(&self.buf) {
                Ok(0) => {
                    return Err(io::Error::new(ErrorKind::WriteZero, "write zero"));
                }
                Ok(n) => {
                    self.buf.drain(..n);
                }
                Err(err) => {
                    let should_close = self.socket.should_close.load(Ordering::Relaxed);
                    match err.kind() {
                        ErrorKind::WouldBlock | ErrorKind::TimedOut if !should_close => continue,
                        _ => return Err(err),
                    }
                }
            }
        }
        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), RtmpStreamError> {
        self.buf.extend_from_slice(data);
        self.write_to_socket()?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), RtmpStreamError> {
        self.write_to_socket()?;
        self.socket.flush()?;
        Ok(())
    }
}

pub(crate) fn configure_socket(socket: &TcpStream) {
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
