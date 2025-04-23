pub(crate) trait Validatable {
    /// Should return `true` if data is alive and can be used.
    /// Most CEF structs have `is_valid` method which can be used for implementing this
    fn is_valid(&mut self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum ValidatedError {
    #[error("Data is no longer valid")]
    NotValid,
}
