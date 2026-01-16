use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

use crate::amf3::AmfValue;

#[derive(Error, Debug)]
pub enum DecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),
    #[error("Insufficient data")]
    InsufficientData,
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,
}

pub(crate) fn decode_amf_values(rtmp_msg_payload: &[u8]) -> Result<Vec<AmfValue>, DecodingError> {
    let mut buf = Bytes::copy_from_slice(rtmp_msg_payload);

    todo!()
}

fn decode_value(mut buf: Bytes) -> Result<(AmfValue, Bytes), DecodingError> {
    if !buf.has_remaining() {
        return Err(DecodingError::InsufficientData);
    }

    let marker = buf.get_u8();

    match marker {
        0x00 => Ok((AmfValue::Undefined, buf)),
        0x01 => Ok((AmfValue::Null, buf)),
        0x02 => Ok((AmfValue::False, buf)),
        0x03 => Ok((AmfValue::True, buf)),
        0x04 => {
            if false {
                todo!("I am not sure if it works this way, I assume U2 encoding");
            }

            let uint_value = decode_u29(&mut buf)?;
            let abs_val = uint_value & 0x0F_FF_FF_FF;
            let sign = (uint_value >> 28) & 0x01;

            let integer_value = match sign {
                0 => abs_val as i32,
                1 => -(1 << 28) + (abs_val as i32),
                _ => unreachable!(),
            };

            Ok((AmfValue::Integer(integer_value), buf))
        }
        0x05 => {
            let double_value = buf.get_f64();
            Ok((AmfValue::Double(double_value), buf))
        }
        _ => Err(DecodingError::UnknownType(marker)),
    }
}

fn decode_u29(buf: &mut Bytes) -> Result<u32, DecodingError> {
    if !buf.has_remaining() {
        return Err(DecodingError::InsufficientData);
    }

    let mut u29 = 0u32;
    for b in 0..4 {
        let byte = buf.get_u8();
        if b < 3 {
            let next_byte_present = (byte & 0x80) >> 7;
            let byte = byte & 0x7F;
            u29 <<= 7;
            u29 |= byte as u32;

            // Break early if flag indicating presence of the next byte is set to 0
            if next_byte_present == 0 {
                break;
            }
        } else {
            u29 <<= 8;
            u29 |= byte as u32;
        }
    }

    Ok(u29)
}
