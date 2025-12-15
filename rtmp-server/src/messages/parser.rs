use bytes::{Buf, Bytes};
use std::collections::HashMap;
use thiserror::Error;

use crate::amf0::Parser as Amf0Parser;
use crate::header::{ChunkMessageHeader, ChunkMessageHeaderError};
use crate::messages::*;

#[derive(Error, Debug)]
pub enum MessageParserError {
    #[error("Header error: {0}")]
    HeaderError(#[from] ChunkMessageHeaderError),
    #[error("AMF parse error")]
    AmfParseError,
    #[error("Insufficient data")]
    InsufficientData,
}

pub struct MessageParser {
    chunk_size: u32,
    prev_headers: HashMap<u32, ChunkMessageHeader>,
    partial_messages: HashMap<u32, PartialMessage>,
}

struct PartialMessage {
    header: ChunkMessageHeader,
    data: Vec<u8>,
}

impl MessageParser {
    pub fn new() -> Self {
        Self {
            chunk_size: 128,
            prev_headers: HashMap::new(),
            partial_messages: HashMap::new(),
        }
    }

    pub fn set_chunk_size(&mut self, size: u32) {
        self.chunk_size = size;
    }

    pub fn parse(
        &mut self,
        data: &[u8],
    ) -> Result<(Vec<(ChunkMessageHeader, RtmpMessage)>, usize), MessageParserError> {
        let mut buf = Bytes::copy_from_slice(data);
        let mut messages = Vec::new();
        let mut consumed = 0;

        while buf.has_remaining() {
            let start_len = buf.len();

            let prev_header = self.get_prev_header_for_parse(&buf);
            let (header, remaining) = match ChunkMessageHeader::parse(buf.clone(), prev_header) {
                Ok(result) => result,
                Err(ChunkMessageHeaderError::InsufficientData) => break,
                Err(e) => return Err(e.into()),
            };

            buf = remaining;

            let partial = self
                .partial_messages
                .entry(header.chunk_stream_id)
                .or_insert_with(|| PartialMessage {
                    header: header.clone(),
                    data: Vec::new(),
                });

            let remaining_bytes = header.msg_length as usize - partial.data.len();
            let chunk_bytes = remaining_bytes.min(self.chunk_size as usize);

            if buf.remaining() < chunk_bytes {
                break;
            }

            let chunk_data = buf.copy_to_bytes(chunk_bytes);
            partial.data.extend_from_slice(&chunk_data);

            consumed += start_len - buf.len();

            if partial.data.len() >= header.msg_length as usize {
                let partial = self
                    .partial_messages
                    .remove(&header.chunk_stream_id)
                    .unwrap();
                let message = Self::parse_message(header.msg_type_id, &partial.data)?;

                self.prev_headers
                    .insert(header.chunk_stream_id, header.clone());
                messages.push((header, message));
            } else {
                partial.header = header.clone();
                self.prev_headers.insert(header.chunk_stream_id, header);
            }
        }

        Ok((messages, consumed))
    }

    fn get_prev_header_for_parse(&self, buf: &Bytes) -> Option<&ChunkMessageHeader> {
        if !buf.has_remaining() {
            return None;
        }
        let first_byte = buf[0];
        let cs_id = first_byte & 0x3F;
        let chunk_stream_id = match cs_id {
            0 if buf.remaining() >= 2 => buf[1] as u32 + 64,
            1 if buf.remaining() >= 3 => buf[2] as u32 * 256 + buf[1] as u32 + 64,
            _ => cs_id as u32,
        };
        self.prev_headers.get(&chunk_stream_id)
    }

    fn parse_message(type_id: u8, data: &[u8]) -> Result<RtmpMessage, MessageParserError> {
        match type_id {
            MSG_SET_CHUNK_SIZE => {
                if data.len() < 4 {
                    return Err(MessageParserError::InsufficientData);
                }
                let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                Ok(RtmpMessage::SetChunkSize(size))
            }
            MSG_ACKNOWLEDGEMENT => {
                if data.len() < 4 {
                    return Err(MessageParserError::InsufficientData);
                }
                let seq = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                Ok(RtmpMessage::Acknowledgement(seq))
            }
            MSG_WINDOW_ACK_SIZE => {
                if data.len() < 4 {
                    return Err(MessageParserError::InsufficientData);
                }
                let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                Ok(RtmpMessage::WindowAcknowledgementSize(size))
            }
            MSG_SET_PEER_BANDWIDTH => {
                if data.len() < 5 {
                    return Err(MessageParserError::InsufficientData);
                }
                let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let limit_type = data[4];
                Ok(RtmpMessage::SetPeerBandwidth { size, limit_type })
            }
            MSG_USER_CONTROL => {
                if data.len() < 2 {
                    return Err(MessageParserError::InsufficientData);
                }
                let event_type = u16::from_be_bytes([data[0], data[1]]);
                Ok(RtmpMessage::UserControl {
                    event_type,
                    data: data[2..].to_vec(),
                })
            }
            MSG_COMMAND_AMF0 => {
                let values =
                    Amf0Parser::parse(data).map_err(|_| MessageParserError::AmfParseError)?;

                let name = match values.first() {
                    Some(crate::amf0::parser::AmfValue::String(s)) => s.clone(),
                    _ => return Err(MessageParserError::AmfParseError),
                };

                let transaction_id = match values.get(1) {
                    Some(crate::amf0::parser::AmfValue::Number(n)) => *n,
                    _ => 0.0,
                };

                Ok(RtmpMessage::Command {
                    name,
                    transaction_id,
                    data: values.into_iter().skip(2).collect(),
                })
            }
            MSG_DATA_AMF0 => {
                let values =
                    Amf0Parser::parse(data).map_err(|_| MessageParserError::AmfParseError)?;
                Ok(RtmpMessage::DataMessage(values))
            }
            MSG_AUDIO => Ok(RtmpMessage::Audio(data.to_vec())),
            MSG_VIDEO => Ok(RtmpMessage::Video(data.to_vec())),
            _ => Ok(RtmpMessage::Unknown {
                type_id,
                data: data.to_vec(),
            }),
        }
    }
}

impl Default for MessageParser {
    fn default() -> Self {
        Self::new()
    }
}
