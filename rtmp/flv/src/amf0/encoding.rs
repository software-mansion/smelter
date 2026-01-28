use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use tracing::warn;

use crate::amf0::*;

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
        AmfValue::Undefined => put_undefined(buf),
        AmfValue::EcmaArray(map) => put_ecma_array(buf, map)?,
        AmfValue::StrictArray(arr) => put_strict_array(buf, arr)?,
        AmfValue::Date {
            unix_time,
            timezone_offset,
        } => put_date(buf, *unix_time, *timezone_offset),
        AmfValue::LongString(s) => put_long_string(buf, s)?,
        AmfValue::TypedObject(_name, _map) => unimplemented!(),
    };
    Ok(())
}

fn put_number(buf: &mut BytesMut, n: f64) {
    buf.put_u8(NUMBER);
    buf.put_f64(n);
}

fn put_bool(buf: &mut BytesMut, b: bool) {
    buf.put_u8(BOOLEAN);
    buf.put_u8(b as u8);
}

fn put_string(buf: &mut BytesMut, s: &str) -> Result<(), EncodingError> {
    if s.len() > u16::MAX as usize {
        return Err(EncodingError::StringTooLong(s.len()));
    }
    buf.put_u8(STRING);
    buf.put_u16(s.len() as u16);
    buf.put_slice(s.as_bytes());
    Ok(())
}

fn put_object(buf: &mut BytesMut, map: &HashMap<String, AmfValue>) -> Result<(), EncodingError> {
    buf.put_u8(OBJECT);
    put_keyval_map(buf, map)?;
    Ok(())
}

fn put_null(buf: &mut BytesMut) {
    buf.put_u8(NULL);
}

fn put_undefined(buf: &mut BytesMut) {
    buf.put_u8(UNDEFINED);
}

fn put_ecma_array(
    buf: &mut BytesMut,
    map: &HashMap<String, AmfValue>,
) -> Result<(), EncodingError> {
    buf.put_u8(ECMA_ARRAY);
    buf.put_u32(map.len() as u32);
    put_keyval_map(buf, map)?;
    Ok(())
}

fn put_strict_array(buf: &mut BytesMut, arr: &[AmfValue]) -> Result<(), EncodingError> {
    if arr.len() > u32::MAX as usize {
        return Err(EncodingError::ArrayTooLong(arr.len()));
    }
    buf.put_u8(STRICT_ARRAY);
    buf.put_u32(arr.len() as u32);
    for value in arr {
        encode_value(buf, value)?;
    }
    Ok(())
}

fn put_date(buf: &mut BytesMut, unix_time: f64, timezone_offset: i16) {
    buf.put_u8(DATE);
    buf.put_f64(unix_time);
    if timezone_offset != 0x00_00 {
        warn!("Timezone offset is not zero.");
    }
    buf.put_i16(timezone_offset);
}

fn put_long_string(buf: &mut BytesMut, s: &str) -> Result<(), EncodingError> {
    if s.len() > u32::MAX as usize {
        return Err(EncodingError::StringTooLong(s.len()));
    }
    buf.put_u8(LONG_STRING);
    buf.put_u32(s.len() as u32);
    buf.put_slice(s.as_bytes());
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
