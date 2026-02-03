use bytes::{Buf, Bytes};

use crate::{DecodingError, amf3::*};

/// Function used to decode AMF3 encoded messages. `amf_bytes` must be a payload of `rtmp` Data
/// or Command message, encoded in AMF3.
pub fn decode_amf3_value(amf_bytes: &[u8]) -> Result<Vec<AmfValue>, DecodingError> {
    let mut buf = Bytes::copy_from_slice(amf_bytes);
    let mut result = vec![];
    let mut decoder = Decoder::new();

    while buf.has_remaining() {
        let amf_value = decoder.decode_value(&mut buf)?;
        result.push(amf_value);
    }

    Ok(result)
}

#[derive(Clone)]
struct Trait {
    class_name: Option<String>,
    dynamic: bool,
    field_names: Vec<String>,
}

#[derive(Default)]
struct Decoder {
    strings: Vec<String>,
    traits: Vec<Trait>,
    complexes: Vec<AmfValue>,
}

impl Decoder {
    fn new() -> Self {
        Self::default()
    }

    fn decode_value(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        if buf.is_empty() {
            return Err(DecodingError::InsufficientData);
        }

        let marker = buf.get_u8();

        match marker {
            UNDEFINED => Ok(AmfValue::Undefined),
            NULL => Ok(AmfValue::Null),
            FALSE => Ok(AmfValue::Boolean(false)),
            TRUE => Ok(AmfValue::Boolean(true)),
            INTEGER => self.decode_integer(buf),
            DOUBLE => self.decode_double(buf),
            STRING => self.decode_string(buf),
            XML_DOC => self.decode_xml_doc(buf),
            DATE => self.decode_date(buf),
            ARRAY => self.decode_array(buf),
            OBJECT => self.decode_object(buf),
            XML => self.decode_xml(buf),
            BYTE_ARRAY => self.decode_byte_array(buf),
            VECTOR_INT => self.decode_int_vec(buf),
            VECTOR_UINT => self.decode_uint_vec(buf),
            VECTOR_DOUBLE => self.decode_double_vec(buf),
            VECTOR_OBJECT => self.decode_object_vec(buf),
            DICTIONARY => self.decode_dictionary(buf),
            _ => Err(DecodingError::UnknownType(marker)),
        }
    }

    fn decode_integer(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        if buf.is_empty() {
            return Err(DecodingError::InsufficientData);
        }

        Ok(AmfValue::Integer(self.decode_i29(buf)?))
    }

    fn decode_double(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        if buf.remaining() < 8 {
            return Err(DecodingError::InsufficientData);
        }

        Ok(AmfValue::Double(buf.get_f64()))
    }

    fn decode_string(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        Ok(AmfValue::String(self.decode_string_raw(buf)?))
    }

    fn decode_xml_doc(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Decoder, buf: &mut Bytes, size: usize| {
            if buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let utf8 = buf.split_to(size);
            let xml = String::from_utf8(utf8.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

            let amf_value = AmfValue::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_date(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Decoder, buf: &mut Bytes, _| {
            if buf.remaining() < 8 {
                return Err(DecodingError::InsufficientData);
            }

            let double_date = buf.get_f64();
            let amf_value = AmfValue::Date(double_date);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_array(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, size: usize| {
            if buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let associative = decoder
                .decode_pairs(buf)?
                .into_iter()
                .collect::<HashMap<_, _>>();
            let dense = (0..size)
                .map(|_| decoder.decode_value(buf))
                .collect::<Result<_, _>>()?;

            Ok(AmfValue::Array { associative, dense })
        };

        self.decode_complex(buf, decode)
    }

    fn decode_object(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, u28: usize| {
            let amf_trait = decoder.decode_object_trait(buf, u28)?;
            let sealed_count = amf_trait.field_names.len();
            let mut fields: Vec<(String, AmfValue)> = amf_trait
                .field_names
                .into_iter()
                .map(|key| Ok((key, decoder.decode_value(buf)?)))
                .collect::<Result<_, _>>()?;

            if amf_trait.dynamic {
                fields.extend(decoder.decode_pairs(buf)?);
            }

            let amf_object = AmfValue::Object {
                class_name: amf_trait.class_name,
                sealed_count,
                values: fields,
            };

            decoder.complexes.push(amf_object.clone());
            Ok(amf_object)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_xml(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, size: usize| {
            if buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let utf8 = buf.split_to(size);
            let xml = String::from_utf8(utf8.to_vec()).map_err(|_| DecodingError::InvalidUtf8)?;

            let amf_value = AmfValue::XmlDoc(xml);
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_byte_array(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, size: usize| {
            if buf.remaining() < size {
                return Err(DecodingError::InsufficientData);
            }

            let byte_array = buf.split_to(size);
            let amf_value = AmfValue::ByteArray(byte_array);

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_int_vec(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| decoder.decode_i29(buf))
                .collect::<Result<_, _>>()?;

            let amf_value = AmfValue::VectorInt {
                fixed_length,
                values,
            };
            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_uint_vec(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, item_count: usize| {
            const ITEM_SIZE: usize = 4;

            if buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| {
                    let uint = decoder.decode_u29(buf)?.0;
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

        self.decode_complex(buf, decode)
    }

    fn decode_double_vec(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, item_count: usize| {
            const ITEM_SIZE: usize = 8;

            if buf.remaining() < item_count * ITEM_SIZE + 1 {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = buf.get_u8() == 0x01;

            let values = (0..(item_count * ITEM_SIZE))
                .map(|_| buf.get_f64())
                .collect();

            let amf_value = AmfValue::VectorDouble {
                fixed_length,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_object_vec(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, item_count: usize| {
            if buf.is_empty() {
                return Err(DecodingError::InsufficientData);
            }

            let fixed_length = buf.get_u8() == 0x01;
            let class_name = decoder.decode_string_raw(buf)?;
            let class_name = if class_name == "*" {
                None
            } else {
                Some(class_name)
            };

            let values = (0..item_count)
                .map(|_| decoder.decode_value(buf))
                .collect::<Result<_, _>>()?;

            let amf_value = AmfValue::VectorObject {
                fixed_length,
                class_name,
                values,
            };

            decoder.complexes.push(amf_value.clone());
            Ok(amf_value)
        };

        self.decode_complex(buf, decode)
    }

    fn decode_dictionary(&mut self, buf: &mut Bytes) -> Result<AmfValue, DecodingError> {
        let decode = |decoder: &mut Self, buf: &mut Bytes, entries_count: usize| {
            if buf.is_empty() {
                return Err(DecodingError::InsufficientData);
            }

            let weak_references = buf.get_u8() == 0x01;

            let entries = (0..entries_count)
                .map(|_| {
                    let key = decoder.decode_value(buf)?;
                    let value = decoder.decode_value(buf)?;
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

        self.decode_complex(buf, decode)
    }

    fn decode_complex<F>(&mut self, buf: &mut Bytes, decode: F) -> Result<AmfValue, DecodingError>
    where
        F: FnOnce(&mut Self, &mut Bytes, usize) -> Result<AmfValue, DecodingError>,
    {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = self.decode_u29(buf)?.0;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let amf_value = match has_value {
            true => {
                let size = u28 as usize;
                decode(self, buf, size)?
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
    fn decode_u29(&self, buf: &mut Bytes) -> Result<(u32, usize), DecodingError> {
        let mut result: u32 = 0;
        let mut next_byte_present = false;
        let mut bytes_used: usize = 0;
        for _ in 0..3 {
            if buf.is_empty() {
                return Err(DecodingError::InsufficientData);
            }

            let byte = buf.get_u8();
            bytes_used += 1;
            result <<= 7;
            result |= (byte & 0x7F) as u32;
            next_byte_present = ((byte >> 7) & 0b1) == 1;
            if !next_byte_present {
                break;
            }
        }
        if next_byte_present {
            if buf.is_empty() {
                return Err(DecodingError::InsufficientData);
            }

            let byte = buf.get_u8();
            bytes_used += 1;
            result <<= 8;
            result |= byte as u32;
        }

        Ok((result, bytes_used))
    }

    fn decode_i29(&self, buf: &mut Bytes) -> Result<i32, DecodingError> {
        let (u29, bytes_used) = self.decode_u29(buf)?;

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

    fn decode_string_raw(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = self.decode_u29(buf)?.0;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let string = match has_value {
            true => {
                let size = u28 as usize;
                if size == 0 {
                    String::new()
                } else {
                    if buf.remaining() < size {
                        return Err(DecodingError::InsufficientData);
                    }

                    let utf8 = buf.split_to(size).to_vec();
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

    fn decode_pairs(&mut self, buf: &mut Bytes) -> Result<Vec<(String, AmfValue)>, DecodingError> {
        let mut pairs = vec![];
        loop {
            let key = self.decode_string_raw(buf)?;
            if key.is_empty() {
                return Ok(pairs);
            }

            let value = self.decode_value(buf)?;
            let pair = (key, value);
            pairs.push(pair);
        }
    }

    fn decode_object_trait(&mut self, buf: &mut Bytes, u28: usize) -> Result<Trait, DecodingError> {
        if (u28 & 0b1) == 0 {
            let trait_idx = u28 >> 1;
            let amf_trait = self
                .traits
                .get(trait_idx)
                .ok_or(DecodingError::OutOfBoundsReference)?
                .clone();
            Ok(amf_trait)
        } else if (u28 & 0b11) != 0 {
            Err(DecodingError::ExternalizableTrait)
        } else {
            let dynamic = (u28 & 0b100) != 0;
            let sealed_members = u28 >> 3;

            let class_name = self.decode_string_raw(buf)?;
            let class_name = if class_name.is_empty() {
                None
            } else {
                Some(class_name)
            };

            let field_names = (0..sealed_members)
                .map(|_| self.decode_string_raw(buf))
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

    use crate::amf3::decoding::Decoder;

    #[test]
    fn test_decode_i29() {
        let decoder = Decoder::new();

        // https://github.com/q191201771/doc/blob/master/spec-amf-file-format-spec.pdf
        // Formatting of the test cases is explained in 1.3.1 and 3.6

        // 32 in 7 bit U2
        let mut one_byte_pos = Bytes::from(vec![0b0010_0000]);
        let decoded_val = decoder
            .decode_i29(&mut one_byte_pos)
            .expect("Failed to decode 1 byte positive.");
        assert_eq!(decoded_val, 32);

        // -63 in 7 bit U2
        let mut one_byte_neg = Bytes::from(vec![0b0100_0001]);
        let decoded_val = decoder
            .decode_i29(&mut one_byte_neg)
            .expect("Failed to decode 1 byte negative.");
        assert_eq!(decoded_val, -63);

        // 143 in 14 bit U2
        let mut two_byte_pos = Bytes::from(vec![0b1000_0001, 0b0000_1111]);
        let decoded_val = decoder
            .decode_i29(&mut two_byte_pos)
            .expect("Failed to decode 2 bytes positive.");
        assert_eq!(decoded_val, 143);

        // -8189 in 14 bit U2
        let mut two_byte_neg = Bytes::from(vec![0b1100_0000, 0b0000_0011]);
        let decoded_val = decoder
            .decode_i29(&mut two_byte_neg)
            .expect("Failed to decode 2 bytes negative.");
        assert_eq!(decoded_val, -8189);

        // 16512 in 21 bit U2
        let mut three_byte_pos = Bytes::from(vec![0b1000_0001, 0b1000_0001, 0b0000_0000]);
        let decoded_val = decoder
            .decode_i29(&mut three_byte_pos)
            .expect("Failed to decode 3 bytes positive.");
        assert_eq!(decoded_val, 16512);

        // -1007172 in 21 bit U2
        let mut three_byte_neg = Bytes::from(vec![0b1100_0010, 0b1100_0011, 0b0011_1100]);
        let decoded_val = decoder
            .decode_i29(&mut three_byte_neg)
            .expect("Failed to decode 3 bytes negative.");
        assert_eq!(decoded_val, -1007172);

        // 176193365 in 29 bit U2
        let mut four_byte_pos =
            Bytes::from(vec![0b1010_1010, 0b1000_0000, 0b1111_1111, 0b_0101_0101]);
        let decoded_val = decoder
            .decode_i29(&mut four_byte_pos)
            .expect("Failed to decode 4 bytes positive.");
        assert_eq!(decoded_val, 176193365);

        // -92242091 in 29 bit U2
        let mut four_byte_neg =
            Bytes::from(vec![0b1110_1010, 0b1000_0000, 0b1111_1111, 0b0101_0101]);
        let decoded_val = decoder
            .decode_i29(&mut four_byte_neg)
            .expect("Failed to decode 4 bytes negative.");
        assert_eq!(decoded_val, -92242091);
    }
}
