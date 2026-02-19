use crate::{error::RtmpError, message::RtmpMessage, protocol::MessageType};
use std::{cmp::min, io::Write, net::TcpStream};

pub struct RtmpMessageWriter {
    stream: TcpStream,
    chunk_size: usize,
}

impl RtmpMessageWriter {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            chunk_size: 128,
        }
    }

    #[allow(unused)]
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }

    pub fn write(&mut self, msg: RtmpMessage) -> Result<(), RtmpError> {
        let msg = msg.into_raw()?;

        let mut offset = 0;
        let total_len = msg.payload.len();

        // Protocol Control Messages (types 1-6) MUST use chunk stream ID 2.
        // Command messages use chunk stream ID 3.
        // Audio uses chunk stream ID 4, Video uses chunk stream ID 5.
        // Data messages use chunk stream ID 6.
        let cs_id: u8 = match msg.msg_type {
            MessageType::SetChunkSize
            | MessageType::AbortMessage
            | MessageType::Acknowledgement
            | MessageType::UserControl
            | MessageType::WindowAckSize
            | MessageType::SetPeerBandwidth => 2,
            MessageType::CommandMessageAmf0 | MessageType::CommandMessageAmf3 => 3,
            MessageType::Audio => 4,
            MessageType::Video => 5,
            MessageType::DataMessageAmf0 | MessageType::DataMessageAmf3 => 6,
        };

        while offset < total_len {
            let chunk_len = min(self.chunk_size, total_len - offset);

            if offset == 0 {
                // Chunk type 0 (full header)
                self.stream.write_all(&[(cs_id & 0x3F)])?;
                self.write_u24_be(msg.timestamp)?;
                self.write_u24_be(total_len as u32)?;
                self.stream.write_all(&[msg.msg_type.into_raw()])?;
                self.write_u32_le(msg.stream_id)?;
            } else {
                // Chunk type 3 (continuation)
                self.stream.write_all(&[0xC0 | (cs_id & 0x3F)])?;
            }

            self.stream
                .write_all(&msg.payload[offset..offset + chunk_len])?;

            offset += chunk_len;
        }

        self.stream.flush()?;
        Ok(())
    }

    fn write_u24_be(&mut self, val: u32) -> Result<(), RtmpError> {
        let bytes = val.to_be_bytes();
        self.stream.write_all(&bytes[1..4])?;
        Ok(())
    }
    fn write_u32_le(&mut self, val: u32) -> Result<(), RtmpError> {
        self.stream.write_all(&val.to_le_bytes())?;
        Ok(())
    }
}
