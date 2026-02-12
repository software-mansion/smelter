use std::{
    collections::VecDeque,
    io::Read,
    net::TcpStream,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crate::error::RtmpError;

pub(crate) struct BufferedReader {
    socket: TcpStream,
    buf: VecDeque<u8>,
    read_buf: Vec<u8>,
    should_close: Arc<AtomicBool>,
}

impl BufferedReader {
    pub(crate) fn new(socket: TcpStream, should_close: Arc<AtomicBool>) -> Self {
        socket
            .set_nonblocking(false)
            .expect("Cannot set blocking tcp input stream");
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("Cannot set read timeout");

        Self {
            socket,
            buf: VecDeque::new(),
            read_buf: vec![0; 65536],
            should_close,
        }
    }

    pub(crate) fn read_until_buffer_size(&mut self, buf_size: usize) -> Result<(), RtmpError> {
        loop {
            if self.buf.len() >= buf_size {
                return Ok(());
            }
            match self.socket.read(&mut self.read_buf) {
                Ok(0) => return Err(RtmpError::UnexpectedEof),
                Ok(read_bytes) => {
                    self.buf.extend(self.read_buf[0..read_bytes].iter());
                }
                Err(err) => {
                    let should_close = self.should_close.load(std::sync::atomic::Ordering::Relaxed);
                    match err.kind() {
                        std::io::ErrorKind::WouldBlock if !should_close => {
                            continue;
                        }
                        std::io::ErrorKind::WouldBlock => return Err(err.into()),
                        _ => {
                            return Err(err.into());
                        }
                    }
                }
            };
        }
    }

    pub(crate) fn data(&self) -> &VecDeque<u8> {
        &self.buf
    }

    pub(crate) fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>, RtmpError> {
        if self.buf.len() < len {
            return Err(RtmpError::InternalBufferError(
                "insufficient data in buffer",
            ));
        }
        Ok(self.buf.drain(0..len).collect())
    }
}
