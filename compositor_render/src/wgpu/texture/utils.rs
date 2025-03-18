pub(crate) fn pad_to_256(value: u32) -> u32 {
    if value % 256 == 0 {
        value
    } else {
        value + (256 - (value % 256))
    }
}
