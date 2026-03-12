use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    AmfDecodingError, AmfEncodingError,
    amf0::{AmfValue, decode_amf_values, encode_amf_values},
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
    pub fn parse(data: Bytes) -> Result<Self, AmfDecodingError> {
        if data.is_empty() {
            return Ok(Self { values: vec![] });
        }

        let amf_values = decode_amf_values(data)?;
        let values = amf_values.into_iter().map(ScriptDataValue::from).collect();
        Ok(Self { values })
    }

    pub fn serialize(&self) -> Result<Bytes, AmfEncodingError> {
        let amf_values: Vec<_> = self.values.iter().cloned().map(Into::into).collect();
        encode_amf_values(&amf_values)
    }
}

impl From<AmfValue> for ScriptDataValue {
    fn from(value: AmfValue) -> Self {
        match value {
            AmfValue::Number(n) => Self::Number(n),
            AmfValue::Boolean(b) => Self::Boolean(b),
            AmfValue::String(s) => Self::String(s),
            AmfValue::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            AmfValue::Null => Self::Null,
            AmfValue::Undefined => Self::Undefined,
            AmfValue::EcmaArray(map) => Self::EcmaArray(
                map.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            AmfValue::StrictArray(array) => {
                Self::StrictArray(array.into_iter().map(Self::from).collect())
            }
            AmfValue::Date {
                unix_time,
                timezone_offset,
            } => Self::Date {
                unix_time,
                timezone_offset,
            },
            AmfValue::LongString(s) => Self::LongString(s),
            AmfValue::TypedObject {
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

impl From<ScriptDataValue> for AmfValue {
    fn from(value: ScriptDataValue) -> Self {
        match value {
            ScriptDataValue::Number(n) => Self::Number(n),
            ScriptDataValue::Boolean(b) => Self::Boolean(b),
            ScriptDataValue::String(s) => Self::String(s),
            ScriptDataValue::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            ScriptDataValue::Null => Self::Null,
            ScriptDataValue::Undefined => Self::Undefined,
            ScriptDataValue::EcmaArray(map) => Self::EcmaArray(
                map.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            ScriptDataValue::StrictArray(array) => {
                Self::StrictArray(array.into_iter().map(Self::from).collect())
            }
            ScriptDataValue::Date {
                unix_time,
                timezone_offset,
            } => Self::Date {
                unix_time,
                timezone_offset,
            },
            ScriptDataValue::LongString(s) => Self::LongString(s),
            ScriptDataValue::TypedObject {
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
