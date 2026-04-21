//! Pure-Rust MPEG-TS (ISO/IEC 13818-1) support.
//!
//! Exposes a streaming [`Demuxer`] and a streaming [`Muxer`] built from the
//! same low-level building blocks ([`packet`], [`pes`], [`psi`],
//! [`stream_type`]).

pub mod crc;
pub mod demuxer;
pub mod error;
pub mod muxer;
pub mod packet;
pub mod pes;
pub mod psi;
pub mod stream_type;

pub use demuxer::{Demuxer, DemuxerEvent, EsPacket, StreamInfo};
pub use error::Error;
pub use muxer::{
    DEFAULT_AUDIO_PID, DEFAULT_PMT_PID, DEFAULT_VIDEO_PID, Muxer, MuxerConfig, MuxerInput,
    MuxerStream,
};
pub use stream_type::StreamType;

/// Size of a single MPEG-TS packet.
pub const TS_PACKET_SIZE: usize = 188;

/// MPEG-TS synchronisation byte at the start of every packet.
pub const TS_SYNC_BYTE: u8 = 0x47;

/// 90 kHz clock used by PTS/DTS timestamps.
pub const TS_CLOCK_HZ: u64 = 90_000;
