pub mod parser;
pub mod tag;

pub use parser::error::ParseError;
pub use parser::{audio::*, video::*};
pub use tag::{audio::*, video::*};
