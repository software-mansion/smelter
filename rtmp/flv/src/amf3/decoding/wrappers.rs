use std::collections::HashMap;

use crate::amf3::AmfValue;

pub struct Array {
    pub associative: HashMap<String, AmfValue>,
    pub dense: Vec<AmfValue>,
}

pub struct Object {
    pub class_name: Option<String>,
    pub sealed_count: usize,
    pub values: Vec<(String, AmfValue)>,
}

pub struct VectorInt {
    pub fixed_length: bool,
    pub values: Vec<i32>,
}

pub struct VectorUInt {
    pub fixed_length: bool,
    pub values: Vec<u32>,
}

pub struct VectorDouble {
    pub fixed_length: bool,
    pub values: Vec<f64>,
}

pub struct VectorObject {
    pub fixed_length: bool,
    pub class_name: String,
    pub values: Vec<AmfValue>,
}

pub struct Dictionary {
    pub weak_references: bool,
    pub values: Vec<(AmfValue, AmfValue)>,
}
