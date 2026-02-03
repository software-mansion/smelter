use bytes::{Buf, Bytes};
use std::collections::HashMap;
use tracing::warn;

use crate::{AmfDecodingError, amf0::*};

const OBJECT_END_MARKER: [u8; 3] = [0x00, 0x00, 0x09];

/// Decode AMF0 encoded messages.
///
/// `amf_bytes` must include whole AMF0 values. It can be a payload of `rtmp` Data or Command message.
pub fn decode_amf0_values(amf_bytes: Bytes) -> Result<Vec<AmfValue>, DecodingError> {
    let decoder = AmfDecoderState::new(amf_bytes);
    decoder.decode_buf()
}

#[derive(Default)]
struct AmfDecoderState {
    buf: Bytes,
    // According to spec (https://rtmp.veriskope.com/pdf/amf0-file-format-specification.pdf),
    // complex types are Object, ECMA Array, Strict Array and Typed Objext.
    complexes: Vec<Amf0Value>,
}

impl AmfDecoderState {
    fn new(amf_bytes: Bytes) -> Self {
        Self {
            buf: amf_bytes,
            ..Default::default()
        }
    }

    fn decode_buf(mut self) -> Result<Vec<AmfValue>, DecodingError> {
        let mut amf_values = vec![];
        while self.buf.has_remaining() {
            amf_values.push(self.decode_value()?);
        }
        Ok(amf_values)
    }

    fn decode_value(&mut self) -> Result<AmfValue, DecodingError> {
        if self.buf.is_empty() {
            return Err(DecodingError::InsufficientData);
        }

        let marker = self.buf.get_u8();

        let amf_value = match marker {
            NUMBER => AmfValue::Number(self.decode_number()?),
            BOOLEAN => AmfValue::Boolean(self.decode_boolean()?),
            STRING => AmfValue::String(self.decode_string()?),
            OBJECT => AmfValue::Object(self.decode_object()?),
            NULL => AmfValue::Null,
            UNDEFINED => AmfValue::Undefined,
            REFERENCE => self.decode_reference()?,
            ECMA_ARRAY => AmfValue::EcmaArray(self.decode_ecma_array()?),
            STRICT_ARRAY => AmfValue::StrictArray(self.decode_strict_array()?),
            DATE => {
                let (unix_time, timezone_offset) = self.decode_date()?;
                AmfValue::Date {
                    unix_time,
                    timezone_offset,
                }
            }
            LONG_STRING => AmfValue::LongString(self.decode_long_string()?),
            TYPED_OBJECT => {
                let (class_name, properties) = self.decode_typed_object()?;
                AmfValue::TypedObject {
                    class_name,
                    properties,
                }
            }

            // TODO add switch to AMF3 (0x11)
            _ => return Err(AmfDecodingError::UnknownType(marker)),
        };
        Ok(amf_value)
    }

    fn decode_number(&mut self) -> Result<f64, DecodingError> {
        if self.buf.remaining() < 8 {
            return Err(DecodingError::InsufficientData);
        }
        let number = self.buf.get_f64();
        Ok(number)
    }

    fn decode_boolean(&mut self) -> Result<bool, DecodingError> {
        if self.buf.remaining() < 1 {
            return Err(DecodingError::InsufficientData);
        }
        let boolean = self.buf.get_u8() == 1;
        Ok(boolean)
    }

    fn decode_string(&mut self) -> Result<String, DecodingError> {
        if self.buf.remaining() < 2 {
            return Err(DecodingError::InsufficientData);
        }
        let size = self.buf.get_u16() as usize;
        if self.buf.remaining() < size {
            return Err(DecodingError::InsufficientData);
        }
        let string_bytes = self.buf.split_to(size);
        let string =
            String::from_utf8(string_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;
        Ok(string)
    }

    fn decode_object(&mut self) -> Result<HashMap<String, AmfValue>, DecodingError> {
        let pairs = self.decode_object_pairs()?;
        self.complexes.push(AmfValue::Object(pairs.clone()));
        Ok(pairs)
    }

    fn decode_reference(&mut self) -> Result<AmfValue, DecodingError> {
        if self.buf.remaining() < 2 {
            return Err(DecodingError::InsufficientData);
        }

        let idx = self.buf.get_u16() as usize;
        let complex = match self.complexes.get(idx) {
            Some(c) => c.clone(),
            None => return Err(AmfDecodingError::OutOfBoundsReference),
        };
        Ok(complex)
    }

    fn decode_ecma_array(&mut self) -> Result<HashMap<String, AmfValue>, DecodingError> {
        if self.buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }
        let _array_size = self.buf.get_u32();
        let pairs = self.decode_object_pairs()?;
        self.complexes.push(AmfValue::EcmaArray(pairs.clone()));
        Ok(pairs)
    }

    fn decode_strict_array(&mut self) -> Result<Vec<AmfValue>, DecodingError> {
        if self.buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }
        let size = self.buf.get_u32() as usize;
        let mut array = Vec::with_capacity(size);

        for _ in 0..size {
            let value = self.decode_value()?;
            array.push(value);
        }

        self.complexes.push(Amf0Value::StrictArray(array.clone()));
        Ok(array)
    }

    fn decode_date(&mut self) -> Result<(f64, i16), DecodingError> {
        if self.buf.remaining() < 10 {
            return Err(DecodingError::InsufficientData);
        }

        let unix_time = self.buf.get_f64();
        let timezone_offset = self.buf.get_i16();
        if timezone_offset != 0x00_00 {
            warn!("Timezone offset is not zero.");
        }

        Ok((unix_time, timezone_offset))
    }

    fn decode_long_string(&mut self) -> Result<String, DecodingError> {
        if self.buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let size = self.buf.get_u32() as usize;
        if self.buf.remaining() < size {
            return Err(DecodingError::InsufficientData);
        }
        let string_bytes = self.buf.split_to(size);
        let string =
            String::from_utf8(string_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;
        Ok(string)
    }

    fn decode_typed_object(
        &mut self,
    ) -> Result<(String, HashMap<String, AmfValue>), DecodingError> {
        if self.buf.remaining() < 2 {
            return Err(DecodingError::InsufficientData);
        }

        let class_name = self.decode_string()?;
        let pairs = self.decode_object_pairs()?;

        self.complexes.push(Amf0Value::TypedObject {
            class_name: class_name.clone(),
            properties: pairs.clone(),
        });
        Ok((class_name, pairs))
    }

    fn decode_object_pairs(&mut self) -> Result<HashMap<String, AmfValue>, DecodingError> {
        let mut pairs = HashMap::new();

        loop {
            if self.buf.remaining() < 3 {
                return Err(DecodingError::InsufficientData);
            }
            if self.buf[..3] == OBJECT_END_MARKER {
                self.buf.advance(3);
                return Ok(pairs);
            }
            let key_size = self.buf.get_u16() as usize;
            if self.buf.remaining() < key_size {
                return Err(DecodingError::InsufficientData);
            }
            let key_bytes: Bytes = self.buf.split_to(key_size);
            let key =
                String::from_utf8(key_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;

            let value = self.decode_value()?;
            pairs.insert(key, value);
        }
    }
}
