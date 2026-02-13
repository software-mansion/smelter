use std::collections::HashMap;

use bytes::Bytes;

use crate::{
    amf0::{Amf0Value, decode_amf0_values},
    amf3::Amf3Value,
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
    ExtendedScriptData(ExtendedScriptDataValue),
}

#[derive(Debug, Clone)]
pub enum ExtendedScriptDataValue {
    Undefined,
    Null,
    Boolean(bool),
    Integer(i32),
    Double(f64),
    String(String),
    XmlDoc(String),
    Date(f64),
    Array {
        associative: HashMap<String, ExtendedScriptDataValue>,
        dense: Vec<ExtendedScriptDataValue>,
    },
    Object {
        class_name: Option<String>,
        sealed_count: usize,
        values: Vec<(String, ExtendedScriptDataValue)>,
    },
    Xml(String),
    ByteArray(Bytes),
    VectorInt {
        fixed_length: bool,
        values: Vec<i32>,
    },
    VectorUInt {
        fixed_length: bool,
        values: Vec<u32>,
    },
    VectorDouble {
        fixed_length: bool,
        values: Vec<f64>,
    },
    VectorObject {
        fixed_length: bool,
        class_name: Option<String>,
        values: Vec<ExtendedScriptDataValue>,
    },
    Dictionary {
        weak_references: bool,
        entries: Vec<(ExtendedScriptDataValue, ExtendedScriptDataValue)>,
    },
}

impl ScriptData {
    pub fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let amf_values = decode_amf0_values(data).map_err(ParseError::Amf0)?;

        let scriptdata_values = amf_values.into_iter().map(ScriptDataValue::from).collect();
        Ok(Self {
            values: scriptdata_values,
        })
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
            Amf0Value::AvmPlus(amf3_value) => {
                ScriptDataValue::ExtendedScriptData(amf3_value.into())
            }
        }
    }
}

impl From<Amf3Value> for ExtendedScriptDataValue {
    fn from(value: Amf3Value) -> Self {
        match value {
            Amf3Value::Undefined => Self::Undefined,
            Amf3Value::Null => Self::Null,
            Amf3Value::Boolean(b) => Self::Boolean(b),
            Amf3Value::Integer(i) => Self::Integer(i),
            Amf3Value::Double(d) => Self::Double(d),
            Amf3Value::String(s) => Self::String(s),
            Amf3Value::XmlDoc(xd) => Self::String(xd),
            Amf3Value::Date(d) => Self::Date(d),
            Amf3Value::Array { associative, dense } => {
                let dense = dense.into_iter().map(Self::from).collect();
                let associative = associative
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect();
                Self::Array { associative, dense }
            }
            Amf3Value::Object {
                class_name,
                sealed_count,
                values,
            } => {
                let values = values
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect();
                Self::Object {
                    class_name,
                    sealed_count,
                    values,
                }
            }
            Amf3Value::Xml(x) => Self::Xml(x),
            Amf3Value::ByteArray(ba) => Self::ByteArray(ba),
            Amf3Value::VectorInt {
                fixed_length,
                values,
            } => Self::VectorInt {
                fixed_length,
                values,
            },
            Amf3Value::VectorUInt {
                fixed_length,
                values,
            } => Self::VectorUInt {
                fixed_length,
                values,
            },
            Amf3Value::VectorDouble {
                fixed_length,
                values,
            } => Self::VectorDouble {
                fixed_length,
                values,
            },
            Amf3Value::VectorObject {
                fixed_length,
                class_name,
                values,
            } => {
                let values = values.into_iter().map(Self::from).collect();
                Self::VectorObject {
                    fixed_length,
                    class_name,
                    values,
                }
            }
            Amf3Value::Dictionary {
                weak_references,
                entries,
            } => {
                let entries = entries
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect();
                Self::Dictionary {
                    weak_references,
                    entries,
                }
            }
        }
    }
}
