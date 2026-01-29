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
        todo!()
    }

    fn decode_string(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        todo!()
    }

    fn decode_xml_doc(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        todo!()
    }

    fn decode_date(&mut self, buf: &mut Bytes) -> Result<f64, DecodingError> {
        todo!()
    }

    fn decode_array(&mut self, buf: &mut Bytes) -> Result<Array, DecodingError> {
        todo!()
    }

    fn decode_object(&mut self, buf: &mut Bytes) -> Result<Object, DecodingError> {
        todo!()
    }

    fn decode_xml(&mut self, buf: &mut Bytes) -> Result<String, DecodingError> {
        todo!()
    }

    fn decode_byte_array(&mut self, buf: &mut Bytes) -> Result<Bytes, DecodingError> {
        todo!()
    }

    fn decode_int_vec(&mut self, buf: &mut Bytes) -> Result<VectorInt, DecodingError> {
        todo!()
    }

    fn decode_uint_vec(&mut self, buf: &mut Bytes) -> Result<VectorUInt, DecodingError> {
        todo!()
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
