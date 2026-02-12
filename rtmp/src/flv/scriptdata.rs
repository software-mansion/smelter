use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    SerializationError,
    amf0::{Amf0Value, decode_amf0_values, encode_amf_values},
    error::ParseError,
};

/// Struct representing flv SCRIPTDATA.
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
    TypedObject {
        class_name: String,
        properties: HashMap<String, ScriptDataValue>,
    },
}

impl ScriptData {
    pub fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let amf_values = decode_amf0_values(data).map_err(ParseError::Amf0Decoding)?;

        let scriptdata_values = amf_values.into_iter().map(ScriptDataValue::from).collect();
        Ok(Self {
            values: scriptdata_values,
        })
    }

    pub fn serialize(&self) -> Result<Bytes, SerializationError> {
        Ok(encode_amf_values(
            &self.values.iter().map(Into::into).collect::<Vec<_>>(),
        )?)
    }
}

impl From<Amf0Value> for ScriptDataValue {
    fn from(value: Amf0Value) -> Self {
        match value {
            Amf0Value::Number(n) => Self::Number(n),
            Amf0Value::Boolean(b) => Self::Boolean(b),
            Amf0Value::String(s) => Self::String(s),
            Amf0Value::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            Amf0Value::Null => Self::Null,
            Amf0Value::Undefined => Self::Undefined,
            Amf0Value::EcmaArray(map) => Self::EcmaArray(
                map.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            Amf0Value::StrictArray(array) => {
                Self::StrictArray(array.into_iter().map(Self::from).collect())
            }
            Amf0Value::Date {
                unix_time,
                timezone_offset,
            } => Self::Date {
                unix_time,
                timezone_offset,
            },
            Amf0Value::LongString(s) => Self::LongString(s),
            Amf0Value::TypedObject {
                class_name,
                properties,
            } => {
                let tag_properties = properties
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect();
                Self::TypedObject {
                    class_name,
                    properties: tag_properties,
                }
            }
        }
    }
}

impl From<&ScriptDataValue> for Amf0Value {
    fn from(value: &ScriptDataValue) -> Self {
        match value {
            ScriptDataValue::Number(n) => Amf0Value::Number(*n),
            ScriptDataValue::Boolean(b) => Amf0Value::Boolean(*b),
            ScriptDataValue::String(s) => Amf0Value::String(s.clone()),
            ScriptDataValue::Object(map) => {
                Amf0Value::Object(map.iter().map(|(k, v)| (k.clone(), v.into())).collect())
            }
            ScriptDataValue::Null => Amf0Value::Null,
            ScriptDataValue::Undefined => Amf0Value::Undefined,
            ScriptDataValue::EcmaArray(map) => {
                Amf0Value::EcmaArray(map.iter().map(|(k, v)| (k.clone(), v.into())).collect())
            }
            ScriptDataValue::StrictArray(arr) => {
                Amf0Value::StrictArray(arr.iter().map(Into::into).collect())
            }
            ScriptDataValue::Date {
                unix_time,
                timezone_offset,
            } => Amf0Value::Date {
                unix_time: *unix_time,
                timezone_offset: *timezone_offset,
            },
            ScriptDataValue::LongString(s) => Amf0Value::LongString(s.clone()),
            ScriptDataValue::TypedObject {
                class_name,
                properties,
            } => Amf0Value::TypedObject {
                class_name: class_name.clone(),
                properties: properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.into()))
                    .collect(),
            },
        }
    }
}
