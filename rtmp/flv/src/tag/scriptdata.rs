use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    amf0::{self, decoding::decode_amf0_values},
    error::ParseError,
};

#[derive(Debug, Clone)]
pub struct ScriptData {
    pub values: Vec<ScriptDataValue>,
}

#[derive(Debug, Clone)]
pub enum ScriptDataValue {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(HashMap<String, ScriptDataValue>),
    Null,
    Undefined,
    EcmaArray(HashMap<String, ScriptDataValue>),
    StrictArray(Vec<ScriptDataValue>),
    Date {
        unix_time: f64,
        timezone_offset: i16,
    },
    LongString(String),
    TypedObject(String, HashMap<String, ScriptDataValue>),
}

impl ScriptData {
    pub fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let amf_values = decode_amf0_values(&data).map_err(ParseError::Amf0)?;

        let scriptdata_values = amf_values.into_iter().map(ScriptDataValue::from).collect();
        Ok(Self {
            values: scriptdata_values,
        })
    }
}

impl From<amf0::Value> for ScriptDataValue {
    fn from(value: amf0::Value) -> Self {
        match value {
            amf0::Value::Number(n) => Self::Number(n),
            amf0::Value::Boolean(b) => Self::Boolean(b),
            amf0::Value::String(s) => Self::String(s),
            amf0::Value::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            amf0::Value::Null => Self::Null,
            amf0::Value::Undefined => Self::Undefined,
            amf0::Value::EcmaArray(map) => Self::EcmaArray(
                map.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            amf0::Value::StrictArray(array) => {
                Self::StrictArray(array.into_iter().map(Self::from).collect())
            }
            amf0::Value::Date {
                unix_time,
                timezone_offset,
            } => Self::Date {
                unix_time,
                timezone_offset,
            },
            amf0::Value::LongString(s) => Self::LongString(s),
            amf0::Value::TypedObject(class_name, obj) => {
                let tag_obj = obj
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect();
                Self::TypedObject(class_name, tag_obj)
            }
        }
    }
}
