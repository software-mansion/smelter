use std::collections::HashMap;

use crate::amf0;

pub enum ScriptDataTag {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(HashMap<String, ScriptDataTag>),
    Null,
    Undefined,
    EcmaArray(HashMap<String, ScriptDataTag>),
    StrictArray(Vec<ScriptDataTag>),
    Date {
        unix_time: f64,
        timezone_offset: i16,
    },
    LongString(String),
    TypedObject(String, HashMap<String, ScriptDataTag>),
}

impl From<amf0::AmfValue> for ScriptDataTag {
    fn from(value: amf0::AmfValue) -> Self {
        match value {
            amf0::AmfValue::Number(n) => Self::Number(n),
            amf0::AmfValue::Boolean(b) => Self::Boolean(b),
            amf0::AmfValue::String(s) => Self::String(s),
            amf0::AmfValue::Object(obj) => Self::Object(
                obj.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            amf0::AmfValue::Null => Self::Null,
            amf0::AmfValue::Undefined => Self::Undefined,
            amf0::AmfValue::EcmaArray(map) => Self::EcmaArray(
                map.into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            amf0::AmfValue::StrictArray(array) => {
                Self::StrictArray(array.into_iter().map(Self::from).collect())
            }
            amf0::AmfValue::Date {
                unix_time,
                timezone_offset,
            } => Self::Date {
                unix_time,
                timezone_offset,
            },
            amf0::AmfValue::LongString(s) => Self::LongString(s),
            amf0::AmfValue::TypedObject(class_name, obj) => {
                let tag_obj = obj
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect();
                Self::TypedObject(class_name, tag_obj)
            }
        }
    }
}
