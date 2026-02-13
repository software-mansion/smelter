use bytes::{Buf, Bytes};

use crate::{AmfDecodingError, amf3::*};

#[allow(dead_code)]
/// Decode AMF3 encoded messages.
///
/// `amf_bytes` must include whole AMF3 values. It can be a payload of `rtmp` Data or Command message.
pub fn decode_amf3_values(amf_bytes: Bytes) -> Result<Vec<Amf3Value>, AmfDecodingError> {
    let decoder: Amf3DecoderState<Bytes> = Amf3DecoderState::new(amf_bytes);
    decoder.decode_buf()
}

#[derive(Clone)]
struct Trait {
    class_name: Option<String>,
    dynamic: bool,
    field_names: Vec<String>,
}

pub(crate) struct Amf3DecoderState<T> {
    buf: T,
    strings: Vec<String>,
    traits: Vec<Trait>,
    complexes: Vec<Amf3Value>,
}

impl<T> Amf3DecoderState<T>
where
    T: Buf,
{
    pub(crate) fn new(amf_buf: T) -> Self {
        Self {
            buf: amf_buf,
            strings: vec![],
            traits: vec![],
            complexes: vec![],
        }
    }

    fn decode_buf(mut self) -> Result<Vec<Amf3Value>, AmfDecodingError> {
        let mut amf_values = vec![];
        while self.buf.has_remaining() {
            amf_values.push(self.decode_value()?);
        }
        Ok(amf_values)
    }

    pub(crate) fn decode_value(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        if !self.buf.has_remaining() {
            return Err(AmfDecodingError::InsufficientData);
        }

        let marker = self.buf.get_u8();

        match marker {
            UNDEFINED => Ok(Amf3Value::Undefined),
            NULL => Ok(Amf3Value::Null),
            FALSE => Ok(Amf3Value::Boolean(false)),
            TRUE => Ok(Amf3Value::Boolean(true)),
            INTEGER => self.decode_integer(),
            DOUBLE => self.decode_double(),
            STRING => self.decode_string(),
            XML_DOC => self.decode_xml_doc(),
            DATE => self.decode_date(),
            ARRAY => self.decode_array(),
            OBJECT => self.decode_object(),
            XML => self.decode_xml(),
            BYTE_ARRAY => self.decode_byte_array(),
            VECTOR_INT => self.decode_int_vec(),
            VECTOR_UINT => self.decode_uint_vec(),
            VECTOR_DOUBLE => self.decode_double_vec(),
            VECTOR_OBJECT => self.decode_object_vec(),
            DICTIONARY => self.decode_dictionary(),
            _ => Err(AmfDecodingError::UnknownType(marker)),
        }
    }

    fn decode_integer(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        if !self.buf.has_remaining() {
            return Err(AmfDecodingError::InsufficientData);
        }

        Ok(Amf3Value::Integer(self.decode_i29()?))
    }

    fn decode_double(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        if self.buf.remaining() < 8 {
            return Err(AmfDecodingError::InsufficientData);
        }

        Ok(Amf3Value::Double(self.buf.get_f64()))
    }

    fn decode_string(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        Ok(Amf3Value::String(self.decode_string_raw()?))
    }

    fn decode_xml_doc(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(AmfDecodingError::InsufficientData);
            }

            let utf8 = decoder.buf.copy_to_bytes(size);
            let xml =
                String::from_utf8(utf8.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;

            let amf_value = Amf3Value::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_date(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, _| {
            if decoder.buf.remaining() < 8 {
                return Err(AmfDecodingError::InsufficientData);
            }

            let double_date = decoder.buf.get_f64();
            let amf_value = Amf3Value::Date(double_date);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_array(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(AmfDecodingError::InsufficientData);
            }

            let associative = decoder
                .decode_pairs()?
                .into_iter()
                .collect::<HashMap<_, _>>();
            let dense = (0..size)
                .map(|_| decoder.decode_value())
                .collect::<Result<_, _>>()?;

            Ok(Amf3Value::Array { associative, dense })
        };

        self.decode_complex(decode)
    }

    fn decode_object(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, u28: usize| {
            let amf_trait = decoder.decode_object_trait(u28)?;
            let sealed_count = amf_trait.field_names.len();
            let mut fields: Vec<(String, Amf3Value)> = amf_trait
                .field_names
                .into_iter()
                .map(|key| Ok((key, decoder.decode_value()?)))
                .collect::<Result<_, _>>()?;

            if amf_trait.dynamic {
                fields.extend(decoder.decode_pairs()?);
            }

            let amf_object = Amf3Value::Object {
                class_name: amf_trait.class_name,
                sealed_count,
                values: fields,
            };

            decoder.complexes.push(amf_object.clone());
            Ok(amf_object)
        };

        self.decode_complex(decode)
    }

    fn decode_xml(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(AmfDecodingError::InsufficientData);
            }

            let utf8 = decoder.buf.copy_to_bytes(size);
            let xml =
                String::from_utf8(utf8.to_vec()).map_err(|_| AmfDecodingError::InvalidUtf8)?;

            let amf_value = Amf3Value::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_byte_array(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(AmfDecodingError::InsufficientData);
            }

            let byte_array = decoder.buf.copy_to_bytes(size);
            let amf_value = Amf3Value::ByteArray(byte_array);

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_int_vec(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(AmfDecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| decoder.decode_i29())
                .collect::<Result<_, _>>()?;

            let amf_value = Amf3Value::VectorInt {
                fixed_length,
                values,
            };
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_uint_vec(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(AmfDecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| {
                    let uint = decoder.decode_u29()?;
                    Ok(uint)
                })
                .collect::<Result<_, _>>()?;

            let amf_value = Amf3Value::VectorUInt {
                fixed_length,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_double_vec(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 8;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(AmfDecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| decoder.buf.get_f64())
                .collect();

            let amf_value = Amf3Value::VectorDouble {
                fixed_length,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_object_vec(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            if !decoder.buf.has_remaining() {
                return Err(AmfDecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;
            let class_name = decoder.decode_string_raw()?;
            let class_name = if class_name == "*" {
                None
            } else {
                Some(class_name)
            };

            let values = (0..item_count)
                .map(|_| decoder.decode_value())
                .collect::<Result<_, _>>()?;

            let amf_value = Amf3Value::VectorObject {
                fixed_length,
                class_name,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_dictionary(&mut self) -> Result<Amf3Value, AmfDecodingError> {
        let decode = |decoder: &mut Self, entries_count: usize| {
            if !decoder.buf.has_remaining() {
                return Err(AmfDecodingError::InsufficientData);
            }

            let weak_references = decoder.buf.get_u8() == 0x01;

            let entries = (0..entries_count)
                .map(|_| {
                    let key = decoder.decode_value()?;
                    let value = decoder.decode_value()?;
                    Ok((key, value))
                })
                .collect::<Result<_, _>>()?;

            let amf_value = Amf3Value::Dictionary {
                weak_references,
                entries,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_complex<F>(&mut self, decode: F) -> Result<Amf3Value, AmfDecodingError>
    where
        F: FnOnce(&mut Self, usize) -> Result<Amf3Value, AmfDecodingError>,
    {
        if self.buf.remaining() < 4 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let u29 = self.decode_u29()?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let amf_value = match has_value {
            true => {
                let size = u28 as usize;
                decode(self, size)?
            }
            false => {
                let idx = u28 as usize;
                self.complexes
                    .get(idx)
                    .ok_or(AmfDecodingError::OutOfBoundsReference)?
                    .clone()
            }
        };

        Ok(amf_value)
    }

    // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
    // Check amf3 spec sections 1.3.1 and 3.6 to learn more about how this serialization works
    fn decode_u29(&mut self) -> Result<u32, AmfDecodingError> {
        let mut result: u32 = 0;
        let mut bytes_used: usize = 0;

        let mut decode_byte = || {
            if !self.buf.has_remaining() {
                return Err(AmfDecodingError::InsufficientData);
            }

            let byte = self.buf.get_u8();
            bytes_used += 1;

            let (shift, value_mask) = match bytes_used {
                1..4 => (7, 0x7F),
                4 => (8, 0xFF),
                _ => unreachable!(),
            };

            result <<= shift;
            result |= (byte & value_mask) as u32;

            let next_byte_present = match bytes_used {
                1..4 => ((byte >> 7) & 0b1) == 1,
                4 => false,
                _ => unreachable!(),
            };
            Ok(next_byte_present)
        };

        while decode_byte()? {}

        Ok(result)
    }

    fn decode_i29(&mut self) -> Result<i32, AmfDecodingError> {
        let u29 = self.decode_u29()?;
        if u29 & (1 << 28) != 0 {
            Ok((u29 as i32) - (1 << 29))
        } else {
            Ok(u29 as i32)
        }
    }

    fn decode_string_raw(&mut self) -> Result<String, AmfDecodingError> {
        if self.buf.remaining() < 4 {
            return Err(AmfDecodingError::InsufficientData);
        }

        let u29 = self.decode_u29()?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let string = match has_value {
            true => {
                let size = u28 as usize;
                if size == 0 {
                    String::new()
                } else {
                    if self.buf.remaining() < size {
                        return Err(AmfDecodingError::InsufficientData);
                    }

                    let utf8 = self.buf.copy_to_bytes(size).to_vec();
                    let string =
                        String::from_utf8(utf8).map_err(|_| AmfDecodingError::InvalidUtf8)?;
                    self.strings.push(string.clone());
                    string
                }
            }
            false => {
                let idx = u28 as usize;
                self.strings
                    .get(idx)
                    .ok_or(AmfDecodingError::OutOfBoundsReference)?
                    .clone()
            }
        };
        Ok(string)
    }

    fn decode_pairs(&mut self) -> Result<Vec<(String, Amf3Value)>, AmfDecodingError> {
        let mut pairs = vec![];
        loop {
            let key = self.decode_string_raw()?;
            if key.is_empty() {
                return Ok(pairs);
            }

            let value = self.decode_value()?;
            let pair = (key, value);
            pairs.push(pair);
        }
    }

    fn decode_object_trait(&mut self, u28: usize) -> Result<Trait, AmfDecodingError> {
        // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
        // Flags explained in section 3.12

        const TRAIT_HAS_VALUE_FLAG: usize = 0b1;
        const TRAIT_EXTERNALIZABLE_FLAG: usize = 0b11;

        if (u28 & TRAIT_HAS_VALUE_FLAG) == 0 {
            let trait_idx = u28 >> 1;
            let amf_trait = self
                .traits
                .get(trait_idx)
                .ok_or(AmfDecodingError::OutOfBoundsReference)?
                .clone();
            Ok(amf_trait)
        } else if (u28 & TRAIT_EXTERNALIZABLE_FLAG) != 0 {
            Err(AmfDecodingError::ExternalizableTrait)
        } else {
            const DYNAMIC_MEMBERS_FLAG: usize = 0b1;

            let trait_marker = u28 >> 2;
            let dynamic = (trait_marker & DYNAMIC_MEMBERS_FLAG) != 0;

            let sealed_members = trait_marker >> 1;

            let class_name = self.decode_string_raw()?;
            let class_name = if class_name.is_empty() {
                None
            } else {
                Some(class_name)
            };

            let field_names = (0..sealed_members)
                .map(|_| self.decode_string_raw())
                .collect::<Result<_, _>>()?;

            let amf_trait = Trait {
                class_name,
                dynamic,
                field_names,
            };

            self.traits.push(amf_trait.clone());
            Ok(amf_trait)
        }
    }
}

#[cfg(test)]
mod decode_test {
    use bytes::Bytes;

    use crate::amf3::decoding::Amf3DecoderState;

    #[test]
    fn test_decode_i29_positive() {
        // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
        // Tested integer representation is explained in 1.3.1 and 3.6

        let mut decoder = Amf3DecoderState::new(Bytes::from_iter([0b01101001]));
        let expected = 105;
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);

        let mut decoder = Amf3DecoderState::new(Bytes::from_iter([0b10010000, 0b01011001]));
        let expected = 2137;
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);

        let mut decoder =
            Amf3DecoderState::new(Bytes::from_iter([0b10111101, 0b10010101, 0b00011001]));
        let expected = 1_002_137;
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);

        let mut decoder = Amf3DecoderState::new(Bytes::from_iter([
            0b10000101, 0b10001100, 0b10011100, 0b11101001,
        ]));
        let expected = 21_372_137;
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_decode_i29_negative() {
        let mut decoder = Amf3DecoderState::new(Bytes::from_iter([
            0b11111111, 0b11111111, 0b11110111, 0b10100111,
        ]));
        let expected = -2137;
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);

        let mut decoder = Amf3DecoderState::new(Bytes::from_iter([
            0b11000000, 0b10000000, 0b10000000, 0b00000000,
        ]));
        let expected = -(1 << 28);
        let actual = decoder.decode_i29().unwrap();
        assert_eq!(actual, expected);
    }
}
