use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum DecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),

    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid UTF-8 string")]
    InvalidUtf8,

    #[error("Complex type reference out of bounds")]
    OutOfBoundsReference,
}

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("String too long: {0} bytes (max {})", u16::MAX)]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {})", u32::MAX)]
    ArrayTooLong(usize),
}
