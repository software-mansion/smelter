mod error;
mod tag;

pub mod amf;

pub use error::*;
pub use tag::{PacketType, audio::*, scriptdata::*, video::*};
