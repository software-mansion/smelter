use thiserror::Error;

use crate::amf3::{I29_MAX, I29_MIN, MAX_SEALED_COUNT, U28_MAX, U29_MAX};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum Amf3EncodingError {
    #[error("String too long: {0} bytes (max {U28_MAX}).")]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {U28_MAX}).")]
    ArrayTooLong(usize),

    #[error("Vector too long: {0} elements (max {U28_MAX}).")]
    VectorTooLong(usize),

    #[error(
        "Sealed count larger than actual number of object members. (Sealed count: {sealed_count}, Actual members: {actual_members})."
    )]
    SealedCountTooLarge {
        sealed_count: usize,
        actual_members: usize,
    },

    #[error("Too many sealed members in an object: {0} elements (max {MAX_SEALED_COUNT}).")]
    SealedMembersCountTooLarge(usize),

    #[error("Dictionary too long: {0} entries (max {U28_MAX}).")]
    DictionaryTooLong(usize),

    #[error("Integer must be in range [{I29_MIN}, {I29_MAX}].")]
    OutOfRangeInteger,

    #[error("U29 must be in range [0, {U29_MAX}].")]
    OutOfRangeU29,
}
