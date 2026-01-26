use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use thiserror::Error;

use crate::amf0::AmfValue;

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("String too long: {0} bytes (max {})", u16::MAX)]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {})", u32::MAX)]
    ArrayTooLong(usize),
}

pub fn encode_amf_values(amf_values: &[AmfValue]) -> Result<Bytes, EncodingError> {
    let mut buf = BytesMut::new();
    for value in amf_values {
        encode_value(&mut buf, value)?;
    }
    Ok(buf.freeze())
}

fn encode_value(buf: &mut BytesMut, value: &AmfValue) -> Result<(), EncodingError> {
    match value {
        AmfValue::Number(n) => put_number(buf, *n),
        AmfValue::Boolean(b) => put_bool(buf, *b),
        AmfValue::String(s) => put_string(buf, s)?,
        AmfValue::Object(map) => put_object(buf, map)?,
        AmfValue::Null => put_null(buf),
        AmfValue::Array(arr) => put_array(buf, arr)?,
        AmfValue::EcmaArray(map) => put_ecma_array(buf, map)?,
    };
    Ok(())
}

fn put_number(buf: &mut BytesMut, n: f64) {
    buf.put_u8(0x00);
    buf.put_f64(n);
}

fn put_bool(buf: &mut BytesMut, b: bool) {
    buf.put_u8(0x01);
    buf.put_u8(b as u8);
}

fn put_string(buf: &mut BytesMut, s: &str) -> Result<(), EncodingError> {
    if s.len() > u16::MAX as usize {
        return Err(EncodingError::StringTooLong(s.len()));
    }
    buf.put_u8(0x02);
    buf.put_u16(s.len() as u16);
    buf.put_slice(s.as_bytes());
    Ok(())
}

fn put_null(buf: &mut BytesMut) {
    buf.put_u8(0x05);
}

fn put_array(buf: &mut BytesMut, arr: &[AmfValue]) -> Result<(), EncodingError> {
    if arr.len() > u32::MAX as usize {
        return Err(EncodingError::ArrayTooLong(arr.len()));
    }
    buf.put_u8(0x0A);
    buf.put_u32(arr.len() as u32);
    for value in arr {
        encode_value(buf, value)?;
    }
    Ok(())
}

fn put_object(buf: &mut BytesMut, map: &HashMap<String, AmfValue>) -> Result<(), EncodingError> {
    buf.put_u8(0x03);
    put_keyval_map(buf, map)?;
    Ok(())
}

fn put_ecma_array(
    buf: &mut BytesMut,
    map: &HashMap<String, AmfValue>,
) -> Result<(), EncodingError> {
    buf.put_u8(0x08);
    buf.put_u32(map.len() as u32);
    put_keyval_map(buf, map)?;
    Ok(())
}

fn put_keyval_map(
    buf: &mut BytesMut,
    map: &HashMap<String, AmfValue>,
) -> Result<(), EncodingError> {
    for (key, value) in map {
        if key.len() > u16::MAX as usize {
            return Err(EncodingError::StringTooLong(key.len()));
        }
        buf.put_u16(key.len() as u16);
        buf.put_slice(key.as_bytes());
        encode_value(buf, value)?;
    }
    put_object_end(buf);
    Ok(())
}

fn put_object_end(buf: &mut BytesMut) {
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x09);
}
