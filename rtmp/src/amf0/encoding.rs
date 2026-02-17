use bytes::{BufMut, Bytes, BytesMut};
use std::collections::HashMap;
use tracing::warn;

use crate::{AmfEncodingError, amf0::*, amf3::Amf3EncoderState};

pub fn encode_amf0_values(amf_values: &[Amf0Value]) -> Result<Bytes, AmfEncodingError> {
    let encoder = Amf0EncoderState::new(BytesMut::new());
    encoder.encode_values(amf_values)
}

pub fn encode_avmplus_values(amf_values: &[Amf0Value]) -> Result<Bytes, AmfEncodingError> {
    let mut buf = BytesMut::new();
    buf.put_u8(0);
    let encoder = Amf0EncoderState::new(buf);
    encoder.encode_values(amf_values)
}

struct Amf0EncoderState {
    buf: BytesMut,
}

impl Amf0EncoderState {
    fn new(buf: BytesMut) -> Self {
        Self { buf }
    }

    fn encode_values(mut self, amf_values: &[Amf0Value]) -> Result<Bytes, AmfEncodingError> {
        for value in amf_values {
            self.encode_value(value)?;
        }
        Ok(self.buf.freeze())
    }

    fn encode_value(&mut self, value: &Amf0Value) -> Result<(), AmfEncodingError> {
        match value {
            Amf0Value::Number(n) => self.put_number(*n),
            Amf0Value::Boolean(b) => self.put_bool(*b),
            Amf0Value::String(s) => self.put_string(s)?,
            Amf0Value::Object(map) => self.put_object(map)?,
            Amf0Value::Null => self.put_null(),
            Amf0Value::Undefined => self.put_undefined(),
            Amf0Value::EcmaArray(map) => self.put_ecma_array(map)?,
            Amf0Value::StrictArray(arr) => self.put_strict_array(arr)?,
            Amf0Value::Date {
                unix_time,
                timezone_offset,
            } => self.put_date(*unix_time, *timezone_offset),
            Amf0Value::LongString(s) => self.put_long_string(s)?,
            Amf0Value::TypedObject {
                class_name: _class_name,
                properties: _properties,
            } => unimplemented!(),
            Amf0Value::AvmPlus(amf3_value) => self.put_avmplus_object(amf3_value)?,
        };
        Ok(())
    }

    fn put_number(&mut self, n: f64) {
        self.buf.put_u8(NUMBER);
        self.buf.put_f64(n);
    }

    fn put_bool(&mut self, b: bool) {
        self.buf.put_u8(BOOLEAN);
        self.buf.put_u8(b.into());
    }

    fn put_string(&mut self, s: &str) -> Result<(), AmfEncodingError> {
        if s.len() > u16::MAX as usize {
            return Err(AmfEncodingError::StringTooLong(s.len()));
        }
        self.buf.put_u8(STRING);
        self.buf.put_u16(s.len() as u16);
        self.buf.put_slice(s.as_bytes());
        Ok(())
    }

    fn put_object(&mut self, map: &HashMap<String, Amf0Value>) -> Result<(), AmfEncodingError> {
        self.buf.put_u8(OBJECT);
        self.put_keyval_map(map)
    }

    fn put_null(&mut self) {
        self.buf.put_u8(NULL);
    }

    fn put_undefined(&mut self) {
        self.buf.put_u8(UNDEFINED);
    }

    fn put_ecma_array(&mut self, map: &HashMap<String, Amf0Value>) -> Result<(), AmfEncodingError> {
        self.buf.put_u8(ECMA_ARRAY);
        self.buf.put_u32(map.len() as u32);
        self.put_keyval_map(map)
    }

    fn put_strict_array(&mut self, arr: &[Amf0Value]) -> Result<(), AmfEncodingError> {
        if arr.len() > u32::MAX as usize {
            return Err(AmfEncodingError::ArrayTooLong(arr.len()));
        }
        self.buf.put_u8(STRICT_ARRAY);
        self.buf.put_u32(arr.len() as u32);
        for value in arr {
            self.encode_value(value)?;
        }
        Ok(())
    }

    fn put_date(&mut self, unix_time: f64, timezone_offset: i16) {
        self.buf.put_u8(DATE);
        self.buf.put_f64(unix_time);
        if timezone_offset != 0 {
            warn!("Timezone offset is not zero.");
        }
        self.buf.put_i16(timezone_offset);
    }

    fn put_long_string(&mut self, s: &str) -> Result<(), AmfEncodingError> {
        if s.len() > u32::MAX as usize {
            return Err(AmfEncodingError::LongStringTooLong(s.len()));
        }
        self.buf.put_u8(LONG_STRING);
        self.buf.put_u32(s.len() as u32);
        self.buf.put_slice(s.as_bytes());
        Ok(())
    }

    fn put_avmplus_object(&mut self, amf3_value: &Amf3Value) -> Result<(), AmfEncodingError> {
        self.buf.put_u8(AVMPLUS_OBJECT);
        let mut amf3_encoder = Amf3EncoderState::new(&mut self.buf);
        amf3_encoder.put_value(amf3_value)
    }

    fn put_keyval_map(&mut self, map: &HashMap<String, Amf0Value>) -> Result<(), AmfEncodingError> {
        for (key, value) in map {
            if key.len() > u16::MAX as usize {
                return Err(AmfEncodingError::StringTooLong(key.len()));
            }
            self.buf.put_u16(key.len() as u16);
            self.buf.put_slice(key.as_bytes());
            self.encode_value(value)?;
        }
        self.put_object_end();
        Ok(())
    }

    fn put_object_end(&mut self) {
        self.buf.put_u8(0x00);
        self.buf.put_u8(0x00);
        self.buf.put_u8(0x09);
    }
}
