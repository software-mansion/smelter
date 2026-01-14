use bytes::{Buf, Bytes};
use std::collections::HashMap;
use thiserror::Error;

use crate::amf0::AmfValue;

#[derive(Error, Debug)]
pub enum DecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),
    #[error("Insufficient data")]
    InsufficientData,
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,
}

const OBJECT_END_MARKER: [u8; 3] = [0x00, 0x00, 0x09];

pub(crate) fn decode_amf_values(rtmp_msg_payload: &[u8]) -> Result<Vec<AmfValue>, DecodingError> {
    let mut buf = Bytes::copy_from_slice(rtmp_msg_payload);
    let mut result = Vec::new();

    while buf.has_remaining() {
        let (value, remaining) = decode_value(buf)?;
        result.push(value);
        buf = remaining;
    }

    Ok(result)
}

fn decode_value(mut buf: Bytes) -> Result<(AmfValue, Bytes), DecodingError> {
    if !buf.has_remaining() {
        return Err(DecodingError::InsufficientData);
    }

    let marker = buf.get_u8();

    match marker {
        // number
        0x00 => {
            if buf.remaining() < 8 {
                return Err(DecodingError::InsufficientData);
            }
            let number = buf.get_f64();
            Ok((AmfValue::Number(number), buf))
        }
        // bool
        0x01 => {
            if buf.remaining() < 1 {
                return Err(DecodingError::InsufficientData);
            }
            let boolean = buf.get_u8() == 1;
            Ok((AmfValue::Boolean(boolean), buf))
        }
        // string
        0x02 => {
            if buf.remaining() < 2 {
                return Err(DecodingError::InsufficientData);
            }
            let size = buf.get_u16() as usize;
            if buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }
            let string_bytes = buf.copy_to_bytes(size);
            let string =
                String::from_utf8(string_bytes.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;
            Ok((AmfValue::String(string), buf))
        }
        // object
        0x03 => {
            let (pairs, remaining) = decode_object_pairs(buf)?;
            Ok((AmfValue::Object(pairs), remaining))
        }
        // null
        0x05 => Ok((AmfValue::Null, buf)),
        // ECMA array
        0x08 => {
            if buf.remaining() < 4 {
                return Err(DecodingError::InsufficientData);
            }
            let _array_size = buf.get_u32();
            let (pairs, remaining) = decode_object_pairs(buf)?;
            Ok((AmfValue::EcmaArray(pairs), remaining))
        }
        // strict array
        0x0A => {
            if buf.remaining() < 4 {
                return Err(DecodingError::InsufficientData);
            }
            let size = buf.get_u32() as usize;
            let mut array = Vec::with_capacity(size);
            let mut current_buf = buf;

            for _ in 0..size {
                let (value, remaining) = decode_value(current_buf)?;
                array.push(value);
                current_buf = remaining;
            }

            Ok((AmfValue::Array(array), current_buf))
        }
        // TODO add switch to AMF3 (0x11)
        _ => Err(DecodingError::UnknownType(marker)),
    }
}

fn decode_object_pairs(
    mut buf: Bytes,
) -> Result<(HashMap<String, AmfValue>, Bytes), DecodingError> {
    let mut pairs = HashMap::new();

    loop {
        if buf.remaining() < 3 {
            return Err(DecodingError::InsufficientData);
        }
        if buf[..3] == OBJECT_END_MARKER {
            buf.advance(3);
            return Ok((pairs, buf));
        }
        let key_size = buf.get_u16() as usize;
        if buf.remaining() < key_size {
            return Err(DecodingError::InsufficientData);
        }
        let key_bytes: Bytes = buf.copy_to_bytes(key_size);
        let key = String::from_utf8(key_bytes.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

        let (value, remaining) = decode_value(buf)?;
        pairs.insert(key, value);
        buf = remaining;
    }
}
