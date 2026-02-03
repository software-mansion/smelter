use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    amf0::{AmfValue, decode_amf0_values},
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
    XmlDoc(String),
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

        let amf_values = decode_amf0_values(&data).map_err(ParseError::Amf0)?;

        let scriptdata_values = amf_values.into_iter().map(ScriptDataValue::from).collect();
        Ok(Self {
            values: scriptdata_values,
        })
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
            AmfValue::XmlDoc(s) => Self::XmlDoc(s),
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
