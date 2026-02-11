use std::collections::HashMap;

use bytes::Bytes;

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
