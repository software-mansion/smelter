mod decoding;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AmfValue {
    Undefined,
    Null,
    Bool(bool),

    // This is signed value, even though in spec it is known as U29
    Integer(i32),
    Double(f64),
    String(String),
}
