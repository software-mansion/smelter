use std::collections::HashMap;

pub mod decoding;
pub mod encoding;

#[derive(Debug, Clone, PartialEq)]
pub enum AmfValue {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(HashMap<String, AmfValue>),
    Null,
    Array(Vec<AmfValue>),
    EcmaArray(HashMap<String, AmfValue>),
}
