mod error;
mod tag;

pub mod amf0;
pub mod amf3;

pub use error::*;
pub use tag::{PacketType, audio::*, scriptdata::*, video::*};
