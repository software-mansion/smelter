use bytes::{Buf, Bytes};
use std::collections::HashMap;
use thiserror::Error;
use tracing::warn;

use crate::amf0::*;

#[derive(Error, Debug)]
pub enum DecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),

    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid UTF-8 string")]
    InvalidUtf8,

    #[error("Complex type reference out of bounds")]
    OutOfBoundsReference,
}

const OBJECT_END_MARKER: [u8; 3] = [0x00, 0x00, 0x09];

pub fn decode_amf_values(rtmp_msg_payload: &[u8]) -> Result<Vec<AmfValue>, DecodingError> {
    let mut buf = Bytes::copy_from_slice(rtmp_msg_payload);
    let mut result = Vec::new();
    let mut decoder = Decoder::new();

    while buf.has_remaining() {
        let value = decoder.decode_value(&mut buf)?;
        result.push(value);
    }

    Ok(result)
}

#[derive(Default)]
struct Decoder {
    // According to spec (https://rtmp.veriskope.com/pdf/amf0-file-format-specification.pdf),
    // complex types are Object, ECMA Array, Strict Array and Typed Objext.
    complexes: Vec<AmfValue>,
}

impl Decoder {
    fn new() -> Self {
        Self::default()
    }

    fn decode_value(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        if !buf.has_remaining() {
            return Err(DecodingError::InsufficientData);
        }

        let marker = buf.get_u8();

        let amf_value = match marker {
            NUMBER => AmfValue::Number(self.decode_number(buf)?),
            BOOLEAN => AmfValue::Boolean(self.decode_boolean(buf)?),
            STRING => AmfValue::String(self.decode_string(buf)?),
            OBJECT => AmfValue::Object(self.decode_object(buf)?),
            NULL => AmfValue::Null,
            UNDEFINED => AmfValue::Undefined,
            REFERENCE => self.decode_reference(buf)?,
            ECMA_ARRAY => AmfValue::EcmaArray(self.decode_ecma_array(buf)?),
            STRICT_ARRAY => AmfValue::StrictArray(self.decode_strict_array(buf)?),
            DATE => {
                let (unix_time, timezone_offset) = self.decode_date(buf)?;
                AmfValue::Date {
                    unix_time,
                    timezone_offset,
                }
            }
            LONG_STRING => AmfValue::LongString(self.decode_long_string(buf)?),
            TYPED_OBJECT => {
                let (class_name, pairs) = self.decode_typed_object(buf)?;
                AmfValue::TypedObject(class_name, pairs)
            }

            // TODO add switch to AMF3 (0x11)
            _ => return Err(DecodingError::UnknownType(marker)),
        };
        Ok(amf_value)
    }

    fn decode_number(&mut self, buf: &mut Bytes) -> Result<f64, DecodingError> {
        if buf.remaining() < 8 {
            return Err(DecodingError::InsufficientData);
        }
        let number = buf.get_f64();
        Ok(number)
    }

    fn decode_boolean(&mut self, buf: &mut Bytes) -> Result<bool, DecodingError> {
        if buf.remaining() < 1 {
            return Err(DecodingError::InsufficientData);
        }
        let boolean = buf.get_u8() == 1;
        Ok(boolean)
    }

    fn decode_string(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
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
        Ok(string)
    }

    fn decode_object(
        &mut self,
        buf: &mut Bytes,
    ) -> Result<HashMap<String, AmfValue>, DecodingError> {
        let pairs = self.decode_object_pairs(buf)?;
        self.complexes.push(AmfValue::Object(pairs.clone()));
        Ok(pairs)
    }

    fn decode_reference(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        if buf.remaining() < 2 {
            return Err(DecodingError::InsufficientData);
        }

        let idx = buf.get_u16() as usize;
        let complex = match self.complexes.get(idx) {
            Some(c) => c.clone(),
            None => return Err(DecodingError::OutOfBoundsReference),
        };
        Ok(complex)
    }

    fn decode_ecma_array(
        &mut self,
        buf: &mut Bytes,
    ) -> Result<HashMap<String, AmfValue>, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }
        let _array_size = buf.get_u32();
        let pairs = self.decode_object_pairs(buf)?;
        self.complexes.push(AmfValue::EcmaArray(pairs.clone()));
        Ok(pairs)
    }

    fn decode_strict_array(&mut self, buf: &mut Bytes) -> Result<Vec<AmfValue>, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }
        let size = buf.get_u32() as usize;
        let mut array = Vec::with_capacity(size);

        for _ in 0..size {
            let value = self.decode_value(buf)?;
            array.push(value);
        }

        self.complexes.push(AmfValue::StrictArray(array.clone()));
        Ok(array)
    }

    fn decode_date(&mut self, buf: &mut Bytes) -> Result<(f64, i16), DecodingError> {
        if buf.remaining() < 10 {
            return Err(DecodingError::InsufficientData);
        }

        let unix_time = buf.get_f64();
        let timezone_offset = buf.get_i16();
        if timezone_offset != 0x00_00 {
            warn!("Timezone offset is not zero.");
        }

        Ok((unix_time, timezone_offset))
    }

    fn decode_long_string(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let size = buf.get_u32() as usize;
        if buf.remaining() < size {
            return Err(DecodingError::InsufficientData);
        }
        let string_bytes = buf.copy_to_bytes(size);
        let string =
            String::from_utf8(string_bytes.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;
        Ok(string)
    }

    fn decode_typed_object(
        &mut self,
        buf: &mut Bytes,
    ) -> Result<(String, HashMap<String, AmfValue>), DecodingError> {
        if buf.remaining() < 2 {
            return Err(DecodingError::InsufficientData);
        }

        let class_name = self.decode_string(buf)?;
        let pairs = self.decode_object_pairs(buf)?;

        self.complexes
            .push(AmfValue::TypedObject(class_name.clone(), pairs.clone()));
        Ok((class_name, pairs))
    }

    fn decode_object_pairs(
        &mut self,
        buf: &mut Bytes,
    ) -> Result<HashMap<String, AmfValue>, DecodingError> {
        let mut pairs = HashMap::new();

        loop {
            if buf.remaining() < 3 {
                return Err(DecodingError::InsufficientData);
            }
            if buf[..3] == OBJECT_END_MARKER {
                buf.advance(3);
                return Ok(pairs);
            }
            let key_size = buf.get_u16() as usize;
            if buf.remaining() < key_size {
                return Err(DecodingError::InsufficientData);
            }
            let key_bytes: Bytes = buf.copy_to_bytes(key_size);
            let key =
                String::from_utf8(key_bytes.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

            let value = self.decode_value(buf)?;
            pairs.insert(key, value);
        }
    }
}
