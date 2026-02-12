use std::collections::HashMap;

use bytes::Bytes;

mod decoding;
mod encoding;

#[cfg(test)]
mod amf3_tests;

pub(crate) use decoding::Amf3DecoderState;
pub(crate) use encoding::Amf3EncoderState;

const UNDEFINED: u8 = 0x00;
const NULL: u8 = 0x01;
const FALSE: u8 = 0x02;
const TRUE: u8 = 0x03;
const INTEGER: u8 = 0x04;
const DOUBLE: u8 = 0x05;
const STRING: u8 = 0x06;
const XML_DOC: u8 = 0x07;
const DATE: u8 = 0x08;
const ARRAY: u8 = 0x09;
const OBJECT: u8 = 0x0A;
const XML: u8 = 0x0B;
const BYTE_ARRAY: u8 = 0x0C;
const VECTOR_INT: u8 = 0x0D;
const VECTOR_UINT: u8 = 0x0E;
const VECTOR_DOUBLE: u8 = 0x0F;
const VECTOR_OBJECT: u8 = 0x10;
const DICTIONARY: u8 = 0x11;

#[derive(Debug, Clone, PartialEq)]
pub enum Amf3Value {
    Undefined,
    Null,
    Boolean(bool),
    Integer(i32),
    Double(f64),
    String(String),
    XmlDoc(String),
    Date(f64),
    Array {
        associative: HashMap<String, Amf3Value>,
        dense: Vec<Amf3Value>,
    },
    Object {
        class_name: Option<String>,
        sealed_count: usize,
        values: Vec<(String, Amf3Value)>,
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
        values: Vec<Amf3Value>,
    },
    Dictionary {
        weak_references: bool,
        entries: Vec<(Amf3Value, Amf3Value)>,
    },
}
