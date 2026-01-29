use bytes::Bytes;

use crate::amf3::AmfValue;

struct Trait {
    class_name: Option<String>,
    dynamic: bool,
    field_names: Vec<String>,
}

#[derive(Default)]
struct Decoder {
    strings: Vec<String>,
    traits: Vec<Trait>,
    complexes: Vec<AmfValue>,
}

impl Decoder {
    fn new() -> Self {
        Self::default()
    }

    fn decode_value(&mut self, buf: &mut Bytes) -> AmfValue {
        // if !buf.has_remaining() {
        //     return Err(Dec);
        // }
        todo!()
    }
}
