use std::fmt::Display;

#[derive(Debug, PartialEq)]
pub struct TypeError(String);

impl Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for TypeError {}

impl TypeError {
    pub fn new<S: Into<String>>(msg: S) -> Self {
        Self(msg.into())
    }
}
