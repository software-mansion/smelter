use bytes::{Buf, Bytes};

use crate::{DecodingError, amf3::*};

/// Decode AMF3 encoded messages.
///
/// `amf_bytes` must include whole AMF3 values. It can be a payload of `rtmp` Data or Command message.
pub fn decode_amf3_values(amf_bytes: Bytes) -> Result<Vec<AmfValue>, DecodingError> {
    let decoder = AmfDecoderState::new(amf_bytes);
    decoder.decode_buf()
}

#[derive(Clone)]
struct Trait {
    class_name: Option<String>,
    dynamic: bool,
    field_names: Vec<String>,
}

#[derive(Default)]
struct AmfDecoderState {
    buf: Bytes,
    strings: Vec<String>,
    traits: Vec<Trait>,
    complexes: Vec<AmfValue>,
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

        match marker {
            UNDEFINED => Ok(AmfValue::Undefined),
            NULL => Ok(AmfValue::Null),
            FALSE => Ok(AmfValue::Boolean(false)),
            TRUE => Ok(AmfValue::Boolean(true)),
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
            _ => Err(DecodingError::UnknownType(marker)),
        }
    }

    fn decode_integer(&mut self) -> Result<AmfValue, DecodingError> {
        if self.buf.is_empty() {
            return Err(DecodingError::InsufficientData);
        }

        Ok(AmfValue::Integer(self.decode_i29()?))
    }

    fn decode_double(&mut self) -> Result<AmfValue, DecodingError> {
        if self.buf.remaining() < 8 {
            return Err(DecodingError::InsufficientData);
        }

        Ok(AmfValue::Double(self.buf.get_f64()))
    }

    fn decode_string(&mut self) -> Result<AmfValue, DecodingError> {
        Ok(AmfValue::String(self.decode_string_raw()?))
    }

    fn decode_xml_doc(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut AmfDecoderState, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let utf8 = decoder.buf.split_to(size);
            let xml = String::from_utf8(utf8.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

            let amf_value = AmfValue::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_date(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut AmfDecoderState, _| {
            if decoder.buf.remaining() < 8 {
                return Err(DecodingError::InsufficientData);
            }

            let double_date = decoder.buf.get_f64();
            let amf_value = AmfValue::Date(double_date);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_array(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let associative = decoder
                .decode_pairs()?
                .into_iter()
                .collect::<HashMap<_, _>>();
            let dense = (0..size)
                .map(|_| decoder.decode_value())
                .collect::<Result<_, _>>()?;

            Ok(AmfValue::Array { associative, dense })
        };

        self.decode_complex(decode)
    }

    fn decode_object(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, u28: usize| {
            let amf_trait = decoder.decode_object_trait(u28)?;
            let sealed_count = amf_trait.field_names.len();
            let mut fields: Vec<(String, AmfValue)> = amf_trait
                .field_names
                .into_iter()
                .map(|key| Ok((key, decoder.decode_value()?)))
                .collect::<Result<_, _>>()?;

            if amf_trait.dynamic {
                fields.extend(decoder.decode_pairs()?);
            }

            let amf_object = AmfValue::Object {
                class_name: amf_trait.class_name,
                sealed_count,
                values: fields,
            };

            decoder.complexes.push(amf_object.clone());
            Ok(amf_object)
        };

        self.decode_complex(decode)
    }

    fn decode_xml(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let utf8 = decoder.buf.split_to(size);
            let xml = String::from_utf8(utf8.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

            let amf_value = AmfValue::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_byte_array(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, size: usize| {
            if decoder.buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let byte_array = decoder.buf.split_to(size);
            let amf_value = AmfValue::ByteArray(byte_array);

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_int_vec(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| decoder.decode_i29())
                .collect::<Result<_, _>>()?;

            let amf_value = AmfValue::VectorInt {
                fixed_length,
                values,
            };
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_uint_vec(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| {
                    let uint = decoder.decode_u29()?.0;
                    Ok(uint)
                })
                .collect::<Result<_, _>>()?;

            let amf_value = AmfValue::VectorUInt {
                fixed_length,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_double_vec(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            const ITEM_SIZE: usize = 8;

            if decoder.buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = decoder.buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| decoder.buf.get_f64())
                .collect();

            let amf_value = AmfValue::VectorDouble {
                fixed_length,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_object_vec(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, item_count: usize| {
            if decoder.buf.is_empty() {
                return Err(DecodingError::InsufficientData);
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

            let amf_value = AmfValue::VectorObject {
                fixed_length,
                class_name,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_dictionary(&mut self) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, entries_count: usize| {
            if decoder.buf.is_empty() {
                return Err(DecodingError::InsufficientData);
            }

            let weak_references = decoder.buf.get_u8() == 0x01;

            let entries = (0..entries_count)
                .map(|_| {
                    let key = decoder.decode_value()?;
                    let value = decoder.decode_value()?;
                    Ok((key, value))
                })
                .collect::<Result<_, _>>()?;

            let amf_value = AmfValue::Dictionary {
                weak_references,
                entries,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(decode)
    }

    fn decode_complex<F>(&mut self, decode: F) -> Result<AmfValue, DecodingError>
    where
        F: FnOnce(&mut Self, usize) -> Result<AmfValue, DecodingError>,
    {
        if self.buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = self.decode_u29()?.0;
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
                    .ok_or(DecodingError::OutOfBoundsReference)?
                    .clone()
            }
        };

        Ok(amf_value)
    }

    // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
    // Check amf3 spec sections 1.3.1 and 3.6 to learn more about how this serialization works
    fn decode_u29(&mut self) -> Result<(u32, usize), DecodingError> {
        let mut result: u32 = 0;
        let mut bytes_used: usize = 0;

        let mut decode_byte = || {
            if self.buf.is_empty() {
                return Err(DecodingError::InsufficientData);
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

        Ok((result, bytes_used))
    }

    fn decode_i29(&mut self) -> Result<i32, DecodingError> {
        let (u29, bytes_used) = self.decode_u29()?;

        let (sign_flag, value_mask): (u32, u32) = match bytes_used {
            1 => (1 << 6, 0x3F),
            2 => (1 << 13, 0x1F_FF),
            3 => (1 << 20, 0x0F_FF_FF),
            4 => (1 << 28, 0x0F_FF_FF_FF),
            _ => unreachable!(),
        };

        let int_val = (u29 & value_mask) as i32;

        let negative = (u29 & sign_flag) > 0;
        match negative {
            false => Ok(int_val),
            true => {
                let min_val = -(sign_flag as i32);
                Ok(min_val + int_val)
            }
        }
    }

    fn decode_string_raw(&mut self) -> Result<String, DecodingError> {
        if self.buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = self.decode_u29()?.0;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let string = match has_value {
            true => {
                let size = u28 as usize;
                if size == 0 {
                    String::new()
                } else {
                    if self.buf.remaining() < size {
                        return Err(DecodingError::InsufficientData);
                    }

                    let utf8 = self.buf.split_to(size).to_vec();
                    let string = String::from_utf8(utf8).map_err(|_| DecodingError::InvalidUtf8)?;
                    self.strings.push(string.clone());
                    string
                }
            }
            false => {
                let idx = u28 as usize;
                self.strings
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?
                    .clone()
            }
        };
        Ok(string)
    }

    fn decode_pairs(&mut self) -> Result<Vec<(String, AmfValue)>, DecodingError> {
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

    fn decode_object_trait(&mut self, u28: usize) -> Result<Trait, DecodingError> {
        // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
        // Flags explained in section 3.12

        const TRAIT_HAS_VALUE_FLAG: usize = 0b1;
        const TRAIT_EXTERNALIZABLE_FLAG: usize = 0b11;

        if (u28 & TRAIT_HAS_VALUE_FLAG) == 0 {
            let trait_idx = u28 >> 1;
            let amf_trait = self
                .traits
                .get(trait_idx)
                .ok_or(DecodingError::OutOfBoundsReference)?
                .clone();
            Ok(amf_trait)
        } else if (u28 & TRAIT_EXTERNALIZABLE_FLAG) != 0 {
            Err(DecodingError::ExternalizableTrait)
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

    use crate::amf3::decoding::AmfDecoderState;

    #[test]
    fn test_decode_i29() {
        // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
        // Tested integer representation is explained in 1.3.1 and 3.6

        // 32 in 7 bit U2
        let one_byte_pos = Bytes::from(vec![0b0010_0000]);
        let mut decoder = AmfDecoderState::new(one_byte_pos);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 1 byte positive.");
        assert_eq!(decoded_val, 32);

        // -63 in 7 bit U2
        let one_byte_neg = Bytes::from(vec![0b0100_0001]);
        let mut decoder = AmfDecoderState::new(one_byte_neg);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 1 byte negative.");
        assert_eq!(decoded_val, -63);

        // 143 in 14 bit U2
        let two_byte_pos = Bytes::from(vec![0b1000_0001, 0b0000_1111]);
        let mut decoder = AmfDecoderState::new(two_byte_pos);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 2 bytes positive.");
        assert_eq!(decoded_val, 143);

        // -8189 in 14 bit U2
        let two_byte_neg = Bytes::from(vec![0b1100_0000, 0b0000_0011]);
        let mut decoder = AmfDecoderState::new(two_byte_neg);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 2 bytes negative.");
        assert_eq!(decoded_val, -8189);

        // 16512 in 21 bit U2
        let three_byte_pos = Bytes::from(vec![0b1000_0001, 0b1000_0001, 0b0000_0000]);
        let mut decoder = AmfDecoderState::new(three_byte_pos);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 3 bytes positive.");
        assert_eq!(decoded_val, 16512);

        // -1007172 in 21 bit U2
        let three_byte_neg = Bytes::from(vec![0b1100_0010, 0b1100_0011, 0b0011_1100]);
        let mut decoder = AmfDecoderState::new(three_byte_neg);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 3 bytes negative.");
        assert_eq!(decoded_val, -1007172);

        // 176193493 in 29 bit U2
        let four_byte_pos = Bytes::from(vec![0b1010_1010, 0b1000_0000, 0b1111_1111, 0b_1101_0101]);
        let mut decoder = AmfDecoderState::new(four_byte_pos);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 4 bytes positive.");
        assert_eq!(decoded_val, 176193493);

        // -92241963 in 29 bit U2
        let four_byte_neg = Bytes::from(vec![0b1110_1010, 0b1000_0000, 0b1111_1111, 0b1101_0101]);
        let mut decoder = AmfDecoderState::new(four_byte_neg);
        let decoded_val = decoder
            .decode_i29()
            .expect("Failed to decode 4 bytes negative.");
        assert_eq!(decoded_val, -92241963);
    }
}
