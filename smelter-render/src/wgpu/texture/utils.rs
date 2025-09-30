pub(crate) fn pad_to_256(value: u32) -> u32 {
    if value.is_multiple_of(256) {
        value
    } else {
        value + (256 - (value % 256))
    }
}
