use super::parser::AmfValue;
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Default)]
pub struct Encoder;

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,

    #[error("String too long: {0} bytes (max {})", u16::MAX)]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {})", u32::MAX)]
    ArrayTooLong(usize),
}

impl Encoder {
    pub fn encode(&self, values: &[AmfValue]) -> Result<Vec<u8>, EncodingError> {
        let mut buf = BytesMut::new();
        for value in values {
            self.encode_value(&mut buf, value)?;
        }
        Ok(buf.to_vec())
    }

    fn encode_value(&self, buf: &mut BytesMut, value: &AmfValue) -> Result<(), EncodingError> {
        match value {
            AmfValue::Number(n) => self.put_number(buf, *n),
            AmfValue::Boolean(b) => self.put_bool(buf, *b),
            AmfValue::String(s) => self.put_string(buf, s)?,
            AmfValue::Object(map) => self.put_object(buf, map)?,
            AmfValue::Null => self.put_null(buf),
            AmfValue::Array(arr) => self.put_array(buf, arr)?,
            AmfValue::EcmaArray(map) => self.put_ecma_array(buf, map)?,
        };
        Ok(())
    }

    #[inline]
    fn put_number(&self, buf: &mut BytesMut, n: f64) {
        buf.put_u8(0x00);
        buf.put_f64(n);
    }

    #[inline]
    fn put_bool(&self, buf: &mut BytesMut, b: bool) {
        buf.put_u8(0x01);
        buf.put_u8(b as u8);
    }

    #[inline]
    fn put_string(&self, buf: &mut BytesMut, s: &str) -> Result<(), EncodingError> {
        if s.len() > u16::MAX as usize {
            return Err(EncodingError::StringTooLong(s.len()));
        }
        buf.put_u8(0x02);
        buf.put_u16(s.len() as u16);
        buf.put_slice(s.as_bytes());
        Ok(())
    }

    #[inline]
    fn put_null(&self, buf: &mut BytesMut) {
        buf.put_u8(0x05);
    }

    fn put_array(&self, buf: &mut BytesMut, arr: &[AmfValue]) -> Result<(), EncodingError> {
        if arr.len() > u32::MAX as usize {
            return Err(EncodingError::ArrayTooLong(arr.len()));
        }
        buf.put_u8(0x0A);
        buf.put_u32(arr.len() as u32);
        for value in arr {
            self.encode_value(buf, value)?;
        }
        Ok(())
    }

    fn put_object(
        &self,
        buf: &mut BytesMut,
        map: &HashMap<String, AmfValue>,
    ) -> Result<(), EncodingError> {
        buf.put_u8(0x03);
        self.put_keyval_map(buf, map)?;
        Ok(())
    }

    fn put_ecma_array(
        &self,
        buf: &mut BytesMut,
        map: &HashMap<String, AmfValue>,
    ) -> Result<(), EncodingError> {
        buf.put_u8(0x08);
        buf.put_u32(map.len() as u32);
        self.put_keyval_map(buf, map)?;
        Ok(())
    }

    fn put_keyval_map(
        &self,
        buf: &mut BytesMut,
        map: &HashMap<String, AmfValue>,
    ) -> Result<(), EncodingError> {
        for (key, value) in map {
            if key.len() > u16::MAX as usize {
                return Err(EncodingError::StringTooLong(key.len()));
            }
            buf.put_u16(key.len() as u16);
            buf.put_slice(key.as_bytes());
            self.encode_value(buf, value)?;
        }
        self.put_object_end(buf);
        Ok(())
    }

    #[inline]
    fn put_object_end(&self, buf: &mut BytesMut) {
        buf.put_u8(0x00);
        buf.put_u8(0x00);
        buf.put_u8(0x09);
    }
}
