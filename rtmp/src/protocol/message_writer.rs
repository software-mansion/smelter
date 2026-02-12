use crate::{error::RtmpError, message::RtmpMessage, protocol::RawMessage};
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
        let msg = RawMessage::try_from(msg)?;

        let mut offset = 0;
        let total_len = msg.payload.len();

        // negeotiaion usually on chunk stream id 2 according to spec
        // https://rtmp.veriskope.com/docs/spec/#54-protocol-control-messages
        const CS_ID: u8 = 2;

        while offset < total_len {
            let chunk_len = min(self.chunk_size, total_len - offset);

            if offset == 0 {
                //  header type 0
                self.stream.write_all(&[(CS_ID & 0x3F)])?;
                // message header
                self.write_u24_be(msg.timestamp)?;
                self.write_u24_be(total_len as u32)?;
                self.stream.write_all(&[msg.msg_type.into_id()])?;
                self.write_u32_le(msg.stream_id)?;
            } else {
                // header type 3
                self.stream.write_all(&[0xC0 | (CS_ID & 0x3F)])?;
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
