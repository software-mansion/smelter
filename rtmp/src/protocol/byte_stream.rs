use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytes::Buf;

use crate::{RtmpStreamError, transport::RtmpTransport};

/// Buffered RTMP socket combining read and write operations on a single transport.
pub(crate) struct RtmpByteStream {
    transport: RtmpTransport,
    reader: BufferedReader,
    writer: BufferedWriter,
}

impl RtmpByteStream {
    pub fn new(transport: RtmpTransport, should_close: Arc<AtomicBool>) -> Self {
        Self {
            transport,
            reader: BufferedReader::new(should_close.clone()),
            writer: BufferedWriter::new(should_close),
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.reader.bytes_read
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), RtmpStreamError> {
        while self.reader.buf.len() < buf.len() {
            self.reader.read(&mut self.transport)?;
        }
        self.reader.buf.copy_to_slice(buf);
        Ok(())
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), RtmpStreamError> {
        self.writer.write(&mut self.transport, data)
    }

    pub fn flush(&mut self) -> Result<(), RtmpStreamError> {
        self.writer.flush(&mut self.transport)
    }

    pub fn get_read_buffer_mut(&mut self) -> &mut VecDeque<u8> {
        &mut self.reader.buf
    }

    /// Try read data from transport into internal buffer
    pub fn try_read(&mut self) -> Result<(), io::Error> {
        self.reader.try_read(&mut self.transport)
    }

    /// Read data from transport into internal buffer
    pub fn read(&mut self) -> Result<(), RtmpStreamError> {
        self.reader.read(&mut self.transport)
    }
}

struct BufferedReader {
    should_close: Arc<AtomicBool>,
    buf: VecDeque<u8>,
    read_buf: Vec<u8>,
    bytes_read: u64,
}

impl BufferedReader {
    fn new(should_close: Arc<AtomicBool>) -> Self {
        Self {
            should_close,
            buf: VecDeque::new(),
            read_buf: vec![0; 65536],
            bytes_read: 0,
        }
    }

    fn read(&mut self, transport: &mut RtmpTransport) -> Result<(), RtmpStreamError> {
        loop {
            let should_close = self.should_close.load(Ordering::Relaxed);
            if let Err(err) = self.try_read(transport) {
                match err.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut if !should_close => {
                        continue;
                    }
                    _ => return Err(err.into()),
                }
            }
            return Ok(());
        }
    }

    fn try_read(&mut self, transport: &mut RtmpTransport) -> Result<(), io::Error> {
        match transport.read(&mut self.read_buf)? {
            0 => Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "connection closed",
            )),
            bytes_read => {
                self.buf.extend(self.read_buf[0..bytes_read].iter());
                self.bytes_read += bytes_read as u64;
                Ok(())
            }
        }
    }
}

struct BufferedWriter {
    should_close: Arc<AtomicBool>,
    buf: Vec<u8>,
}

impl BufferedWriter {
    fn new(should_close: Arc<AtomicBool>) -> Self {
        Self {
            should_close,
            buf: Vec::new(),
        }
    }

    fn write_to_transport(&mut self, transport: &mut RtmpTransport) -> Result<(), io::Error> {
        while !self.buf.is_empty() {
            match transport.write(&self.buf) {
                Ok(0) => {
                    return Err(io::Error::new(ErrorKind::WriteZero, "write zero"));
                }
                Ok(n) => {
                    self.buf.drain(..n);
                }
                Err(err) => {
                    let should_close = self.should_close.load(Ordering::Relaxed);
                    match err.kind() {
                        ErrorKind::WouldBlock | ErrorKind::TimedOut if !should_close => continue,
                        _ => return Err(err),
                    }
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, transport: &mut RtmpTransport, data: &[u8]) -> Result<(), RtmpStreamError> {
        self.buf.extend_from_slice(data);
        self.write_to_transport(transport)?;
        Ok(())
    }

    fn flush(&mut self, transport: &mut RtmpTransport) -> Result<(), RtmpStreamError> {
        self.write_to_transport(transport)?;
        transport.flush()?;
        Ok(())
    }
}
