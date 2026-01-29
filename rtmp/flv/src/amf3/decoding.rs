use bytes::{Buf, Bytes};

use crate::{DecodingError, amf3::*};
use wrappers::*;

mod wrappers;

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
        if !buf.has_remaining() {
            return Err(DecodingError::InsufficientData);
        }

        let marker = buf.get_u8();

        let amf_value = match marker {
            UNDEFINED => AmfValue::Undefined,
            NULL => AmfValue::Null,
            FALSE => AmfValue::Boolean(false),
            TRUE => AmfValue::Boolean(true),
            INTEGER => AmfValue::Integer(self.decode_integer(buf)?),
            DOUBLE => AmfValue::Double(self.decode_double(buf)?),
            STRING => AmfValue::String(self.decode_string(buf)?),
            XML_DOC => AmfValue::XmlDoc(self.decode_xml_doc(buf)?),
            DATE => AmfValue::Date(self.decode_date(buf)?),
            ARRAY => {
                let Array { associative, dense } = self.decode_array(buf)?;
                AmfValue::Array { associative, dense }
            }
            OBJECT => {
                let Object {
                    class_name,
                    sealed_count,
                    values,
                } = self.decode_object(buf)?;
                AmfValue::Object {
                    class_name,
                    sealed_count,
                    values,
                }
            }
            XML => AmfValue::Xml(self.decode_xml(buf)?),
            BYTE_ARRAY => AmfValue::ByteArray(self.decode_byte_array(buf)?),
            VECTOR_INT => {
                let VectorInt {
                    fixed_length,
                    values,
                } = self.decode_int_vec(buf)?;
                AmfValue::VectorInt {
                    fixed_length,
                    values,
                }
            }
            VECTOR_UINT => {
                let VectorUInt {
                    fixed_length,
                    values,
                } = self.decode_uint_vec(buf)?;
                AmfValue::VectorUInt {
                    fixed_length,
                    values,
                }
            }
            VECTOR_DOUBLE => {
                let VectorDouble {
                    fixed_length,
                    values,
                } = self.decode_double_vec(buf)?;
                AmfValue::VectorDouble {
                    fixed_length,
                    values,
                }
            }
            VECTOR_OBJECT => {
                let VectorObject {
                    fixed_length,
                    class_name,
                    values,
                } = self.decode_object_vec(buf)?;
                AmfValue::VectorObject {
                    fixed_length,
                    class_name,
                    values,
                }
            }
            DICTIONARY => {
                let Dictionary {
                    weak_references,
                    values,
                } = self.decode_dictionary(buf)?;
                AmfValue::Dictionary {
                    weak_references,
                    values,
                }
            }
            _ => return Err(DecodingError::UnknownType(marker)),
        };
        Ok(amf_value)
    }

    fn decode_integer(&mut self, buf: &mut Bytes) -> Result<i32, DecodingError> {
        todo!()
    }

    fn decode_double(&mut self, buf: &mut Bytes) -> Result<f64, DecodingError> {
        if buf.remaining() < 8 {
            return Err(DecodingError::InsufficientData);
        }

        Ok(buf.get_f64())
    }

    fn decode_string(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let string = match has_value {
            true => {
                let size = u28 as usize;
                if buf.remaining() < size {
                    return Err(DecodingError::InsufficientData);
                }

                let utf8 = buf.copy_to_bytes(size).to_vec();
                let string = String::from_utf8(utf8).map_err(|_| DecodingError::InvalidUtf8)?;
                self.strings.push(string.clone());
                string
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

    fn decode_xml_doc(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let xml = match has_value {
            true => {
                let size = u28 as usize;
                if buf.remaining() < size {
                    return Err(DecodingError::InsufficientData);
                }

                let utf8 = buf.copy_to_bytes(size).to_vec();
                let xml = String::from_utf8(utf8).map_err(|_| DecodingError::InvalidUtf8)?;
                self.complexes.push(AmfValue::XmlDoc(xml.clone()));
                xml
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::XmlDoc(xml) => xml.clone(),
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(xml)
    }

    fn decode_date(&mut self, buf: &mut Bytes) -> Result<f64, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let date = match has_value {
            true => {
                let date = self.decode_double(buf)?;
                self.complexes.push(AmfValue::Date(date));
                date
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::Date(date) => *date,
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(date)
    }

    fn decode_array(&mut self, buf: &mut Bytes) -> Result<Array, DecodingError> {
        todo!()
    }

    fn decode_object(&mut self, buf: &mut Bytes) -> Result<Object, DecodingError> {
        todo!()
    }

    fn decode_xml(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let xml = match has_value {
            true => {
                let size = u28 as usize;
                if buf.remaining() < size {
                    return Err(DecodingError::InsufficientData);
                }

                let utf8 = buf.copy_to_bytes(size).to_vec();
                let xml = String::from_utf8(utf8).map_err(|_| DecodingError::InvalidUtf8)?;
                self.complexes.push(AmfValue::XmlDoc(xml.clone()));
                xml
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::XmlDoc(xml) => xml.clone(),
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(xml)
    }

    fn decode_byte_array(&mut self, buf: &mut Bytes) -> Result<Bytes, DecodingError> {
        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let bytes = match has_value {
            true => {
                let size = u28 as usize;
                if buf.remaining() < size {
                    return Err(DecodingError::InsufficientData);
                }

                let bytes = buf.copy_to_bytes(size);
                self.complexes.push(AmfValue::ByteArray(bytes.clone()));
                bytes
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::ByteArray(bytes) => bytes.clone(),
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(bytes)
    }

    fn decode_int_vec(&mut self, buf: &mut Bytes) -> Result<VectorInt, DecodingError> {
        const ITEM_SIZE: usize = 4;

        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let vec = match has_value {
            true => {
                let items = u28 as usize;
                if buf.remaining() < items * ITEM_SIZE + 1 {
                    return Err(DecodingError::InsufficientData);
                }

                let fixed_length = buf.get_u8() != 0x00;

                let mut vec_bytes = buf.split_to(items * ITEM_SIZE);
                let mut values = Vec::with_capacity(items * ITEM_SIZE);

                while vec_bytes.has_remaining() {
                    let int = self.decode_integer(&mut vec_bytes)?;
                    values.push(int);
                }

                self.complexes.push(AmfValue::VectorInt {
                    fixed_length,
                    values: values.clone(),
                });

                VectorInt {
                    fixed_length,
                    values,
                }
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::VectorInt {
                        fixed_length,
                        values,
                    } => VectorInt {
                        fixed_length: *fixed_length,
                        values: values.clone(),
                    },
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(vec)
    }

    fn decode_uint_vec(&mut self, buf: &mut Bytes) -> Result<VectorUInt, DecodingError> {
        const ITEM_SIZE: usize = 4;

        if buf.remaining() < 4 {
            return Err(DecodingError::InsufficientData);
        }

        let u29 = decode_u29(buf)?;
        let has_value = (u29 & 0b1) == 1;
        let u28 = u29 >> 1;

        let vec = match has_value {
            true => {
                let items = u28 as usize;
                if buf.remaining() < items * ITEM_SIZE + 1 {
                    return Err(DecodingError::InsufficientData);
                }

                let fixed_length = buf.get_u8() != 0x00;

                let mut vec_bytes = buf.split_to(items * ITEM_SIZE);
                let mut values = Vec::with_capacity(items * ITEM_SIZE);

                while vec_bytes.has_remaining() {
                    let uint = decode_u29(&mut vec_bytes)?;
                    values.push(uint);
                }

                self.complexes.push(AmfValue::VectorUInt {
                    fixed_length,
                    values: values.clone(),
                });

                VectorUInt {
                    fixed_length,
                    values,
                }
            }
            false => {
                let idx = u28 as usize;
                let ref_value = self
                    .complexes
                    .get(idx)
                    .ok_or(DecodingError::OutOfBoundsReference)?;

                match ref_value {
                    AmfValue::VectorUInt {
                        fixed_length,
                        values,
                    } => VectorUInt {
                        fixed_length: *fixed_length,
                        values: values.clone(),
                    },
                    _ => return Err(DecodingError::InvalidReferenceType),
                }
            }
        };
        Ok(vec)
    }

    fn decode_double_vec(&mut self, buf: &mut Bytes) -> Result<VectorDouble, DecodingError> {
        todo!()
    }

    fn decode_object_vec(&mut self, buf: &mut Bytes) -> Result<VectorObject, DecodingError> {
        todo!()
    }

    fn decode_dictionary(&mut self, buf: &mut Bytes) -> Result<Dictionary, DecodingError> {
        todo!()
    }
}

fn decode_u29(buf: &mut Bytes) -> Result<u32, DecodingError> {
    let mut result: u32 = 0;
    let mut next_byte_present = false;
    for _ in 0..3 {
        if buf.is_empty() {
            return Err(DecodingError::InsufficientData);
        }

        let byte = buf.get_u8();
        result <<= 8;
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
        result <<= 8;
        result |= byte as u32;
    }

    Ok(result)
}
