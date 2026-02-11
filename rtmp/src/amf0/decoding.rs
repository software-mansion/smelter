use bytes::{Buf, Bytes};
use std::collections::HashMap;
use tracing::warn;

use crate::{AmfDecodingError, amf0::*, amf3::Amf3DecoderState};

/// Decode AMF0 encoded messages.
///
/// `amf_bytes` must include whole AMF0 values. It can be a payload of `rtmp` Data or Command message.
pub fn decode_amf0_values(amf_bytes: Bytes) -> Result<Vec<Amf0Value>, AmfDecodingError> {
    let decoder = Amf0DecoderState::new(amf_bytes);
    decoder.decode_buf()
}

struct Amf0DecoderState<T> {
    buf: T,
    // According to spec (https://rtmp.veriskope.com/pdf/amf0-file-format-specification.pdf),
    // complex types are Object, ECMA Array, Strict Array and Typed Objext.
    complexes: Vec<Amf0Value>,
}

impl<T> Amf0DecoderState<T>
where
    T: Buf,
{
    fn new(amf_bytes: T) -> Self {
        Self {
            buf: amf_bytes,
            complexes: vec![],
        }
    }

    fn decode_buf(mut self) -> Result<Vec<Amf0Value>, AmfDecodingError> {
        let mut amf_values = vec![];
        while self.buf.has_remaining() {
            amf_values.push(self.decode_value()?);
        }
        Ok(amf_values)
    }

    fn decode_value(&mut self) -> Result<Amf0Value, AmfDecodingError> {
        if !self.buf.has_remaining() {
            return Err(AmfDecodingError::InsufficientData);
        }

        let marker = self.buf.get_u8();

        let amf_value = match marker {
            NUMBER => Amf0Value::Number(self.decode_number()?),
            BOOLEAN => Amf0Value::Boolean(self.decode_boolean()?),
            STRING => Amf0Value::String(self.decode_string()?),
            OBJECT => Amf0Value::Object(self.decode_object()?),
            NULL => Amf0Value::Null,
            UNDEFINED => Amf0Value::Undefined,
            REFERENCE => self.decode_reference()?,
            ECMA_ARRAY => Amf0Value::EcmaArray(self.decode_ecma_array()?),
            STRICT_ARRAY => Amf0Value::StrictArray(self.decode_strict_array()?),
            DATE => {
                let (unix_time, timezone_offset) = self.decode_date()?;
                Amf0Value::Date {
                    unix_time,
                    timezone_offset,
                }
            }
            LONG_STRING => Amf0Value::LongString(self.decode_long_string()?),
            TYPED_OBJECT => {
                let (class_name, properties) = self.decode_typed_object()?;
                Amf0Value::TypedObject {
                    class_name,
                    properties,
                }
            }
            AVMPLUS_OBJECT => Amf0Value::AvmPlus(self.decode_avmplus_object()?),
            _ => return Err(AmfDecodingError::UnknownType(marker)),
        };
        Ok(amf_value)
    }

    fn decode_number(&mut self) -> Result<f64, AmfDecodingError> {
        if self.buf.remaining() < 8 {
            return Err(AmfDecodingError::InsufficientData);
        }
        let number = self.buf.get_f64();
        Ok(number)
    }

    fn decode_boolean(&mut self) -> Result<bool, AmfDecodingError> {
        if self.buf.remaining() < 1 {
            return Err(AmfDecodingError::InsufficientData);
        }
        let boolean = self.buf.get_u8() == 1;
        Ok(boolean)
    }

    fn decode_string(&mut self) -> Result<String, AmfDecodingError> {
        if self.buf.remaining() < 2 {
            return Err(AmfDecodingError::InsufficientData);
        }
        let size = self.buf.get_u16() as usize;
        if self.buf.remaining() < size {
            return Err(AmfDecodingError::InsufficientData);
        }
        let string_bytes = self.buf.copy_to_bytes(size);
        let string =
            String::from_utf8(string_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;
        Ok(string)
    }

    fn decode_object(&mut self) -> Result<HashMap<String, Amf0Value>, AmfDecodingError> {
        let pairs = self.decode_object_pairs()?;
        self.complexes.push(Amf0Value::Object(pairs.clone()));
        Ok(pairs)
    }

    fn decode_reference(&mut self) -> Result<Amf0Value, AmfDecodingError> {
        if self.buf.remaining() < 2 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let idx = self.buf.get_u16() as usize;
        let complex = match self.complexes.get(idx) {
            Some(c) => c.clone(),
            None => return Err(AmfDecodingError::OutOfBoundsReference),
        };
        Ok(complex)
    }

    fn decode_ecma_array(&mut self) -> Result<HashMap<String, Amf0Value>, AmfDecodingError> {
        if self.buf.remaining() < 4 {
            return Err(AmfDecodingError::InsufficientData);
        }
        let _array_size = self.buf.get_u32();
        let pairs = self.decode_object_pairs()?;
        self.complexes.push(Amf0Value::EcmaArray(pairs.clone()));
        Ok(pairs)
    }

    fn decode_strict_array(&mut self) -> Result<Vec<Amf0Value>, AmfDecodingError> {
        if self.buf.remaining() < 4 {
            return Err(AmfDecodingError::InsufficientData);
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

    fn decode_date(&mut self) -> Result<(f64, i16), AmfDecodingError> {
        if self.buf.remaining() < 10 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let unix_time = self.buf.get_f64();
        let timezone_offset = self.buf.get_i16();
        if timezone_offset != 0x00_00 {
            warn!("Timezone offset is not zero.");
        }

        Ok((unix_time, timezone_offset))
    }

    fn decode_long_string(&mut self) -> Result<String, AmfDecodingError> {
        if self.buf.remaining() < 4 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let size = self.buf.get_u32() as usize;
        if self.buf.remaining() < size {
            return Err(AmfDecodingError::InsufficientData);
        }
        let string_bytes = self.buf.copy_to_bytes(size);
        let string =
            String::from_utf8(string_bytes.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;
        Ok(string)
    }

    fn decode_typed_object(
        &mut self,
    ) -> Result<(String, HashMap<String, Amf0Value>), AmfDecodingError> {
        if self.buf.remaining() < 2 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let class_name = self.decode_string()?;
        let pairs = self.decode_object_pairs()?;

        self.complexes.push(Amf0Value::TypedObject {
            class_name: class_name.clone(),
            properties: pairs.clone(),
        });
        Ok((class_name, pairs))
    }

    fn decode_avmplus_object(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let mut amf3_decoder = Amf3DecoderState::new(&mut self.buf);
        amf3_decoder.decode_value()
    }

    fn decode_object_pairs(&mut self) -> Result<HashMap<String, Amf0Value>, AmfDecodingError> {
        let mut pairs = HashMap::new();

        loop {
            if self.buf.remaining() < 3 {
                return Err(AmfDecodingError::InsufficientData);
            }
            let key = self.decode_string()?;
            if key.is_empty() {
                let marker = self.buf.get_u8();
                if marker == OBJECT_END {
                    return Ok(pairs);
                } else {
                    return Err(AmfDecodingError::InvalidObjectEnd);
                }
            }
            let value = self.decode_value()?;
            pairs.insert(key, value);
        }
    }
}
