pub mod parser;
pub mod tag;

pub use parser::error::ParseError;
pub use parser::{Parser, audio::*, video::*};
pub use tag::{FlvTag, Header, PacketType, ScriptDataTag};
pub use tag::{audio::*, video::*};
