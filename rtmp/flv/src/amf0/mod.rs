use std::collections::HashMap;

mod decoding;
mod encoding;

pub use decoding::decode_amf_values;
pub use encoding::encode_amf_values;

const NUMBER: u8 = 0x00;
const BOOLEAN: u8 = 0x01;
const STRING: u8 = 0x02;
const OBJECT: u8 = 0x03;
const NULL: u8 = 0x05;
const UNDEFINED: u8 = 0x06;
const REFERENCE: u8 = 0x07;
const ECMA_ARRAY: u8 = 0x08;
const STRICT_ARRAY: u8 = 0x0A;
const DATE: u8 = 0x0B;
const LONG_STRING: u8 = 0x0C;
const TYPED_OBJECT: u8 = 0x10;

#[derive(Debug, Clone, PartialEq)]
pub enum AmfValue {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(HashMap<String, AmfValue>),
    Null,
    Undefined,
    EcmaArray(HashMap<String, AmfValue>),
    StrictArray(Vec<AmfValue>),
    Date {
        unix_time: f64,
        timezone_offset: i16,
    },
    LongString(String),
    TypedObject {
        class_name: String,
        properties: HashMap<String, AmfValue>,
    },
}
