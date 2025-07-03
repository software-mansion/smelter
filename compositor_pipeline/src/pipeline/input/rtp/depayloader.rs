use std::{mem, time::Duration};

use bytes::Bytes;
use log::error;
use rtp::{
    codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet},
    packetizer::Depacketizer,
};

use crate::pipeline::{
    decoder::AacDepayloaderMode,
    output::rtp::RtpPacket,
    types::{AudioCodec, EncodedChunk, EncodedChunkKind, IsKeyframe, VideoCodec},
};

use self::aac::AacDepayloaderNewError;

use super::DepayloadingError;

pub use aac::{AacDepayloader, AacDepayloadingError};

mod aac;

pub enum DepayloadedCodec {
    H264,
    Vp8,
    Vp9,
    Opus,
    Aac(AacDepayloaderMode, Bytes),
}

pub struct DepayloaderOptions {
    pub codec: DepayloadedCodec,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub mtu: usize,
    pub ssrc: u32,
}

pub(crate) struct Depayloader {
    depayloader: Box<dyn DepayloaderExt>,
    ssrc: u32,
    payload_type: u8,
}

impl Depayloader {
    fn new(options: DepayloaderOptions) -> Result<Self, DepayloaderInitError> {
        let depayloader = match options.codec {
            DepayloadedCodec::H264 => {
                BufferedDepayloader::new::<H264Packet>(EncodedChunkKind::Video(VideoCodec::H264))
            }
            DepayloadedCodec::Vp8 => {
                BufferedDepayloader::new::<Vp8Packet>(EncodedChunkKind::Video(VideoCodec::VP8))
            }
            DepayloadedCodec::Vp9 => {
                BufferedDepayloader::new::<Vp9Packet>(EncodedChunkKind::Video(VideoCodec::VP9))
            }
            DepayloadedCodec::Opus => {
                BufferedDepayloader::new::<OpusPacket>(EncodedChunkKind::Audio(AudioCodec::Opus))
            }
            DepayloadedCodec::Aac(mode, asc) => Box::new(AacDepayloader::new(mode, &asc))?,
        };
        Ok(Self {
            ssrc: options.ssrc,
            depayloader,
            payload_type: options.payload_type,
            depayloader,
        })
    }

    pub fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        self.depayload(packet)
    }
}

trait DepayloaderExt {
    fn depayload(
        &mut self,
        packet: rtp::packet::Packet,
    ) -> Result<Vec<EncodedChunk>, DepayloadingError>;
}

struct BufferedDepayloader<T: rtp::packetizer::Depacketizer + Default> {
    kind: EncodedChunkKind,
    buffer: Vec<Bytes>,
    rollover_state: RolloverState,
    depacketizer: T,
}

impl<T: rtp::packetizer::Depacketizer + Default> BufferedDepayloader<T> {
    fn new(kind: EncodedChunkKind) -> Box<dyn DepayloaderExt> {
        Box::new(Self {
            kind,
            buffer: Vec::new(),
            rollover_state: RolloverState::default(),
            depacketizer: T::default(),
        })
    }
}

impl<T: rtp::packetizer::Depacketizer + Default> DepayloaderExt for BufferedDepayloader<T> {
    fn depayload(
        &mut self,
        packet: rtp::packet::Packet,
    ) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        let chunk = self.depayloader.depacketize(&packet.payload)?;

        if chunk.is_empty() {
            return Ok(Vec::new());
        }

        self.buffer.push(chunk);
        if !packet.header.marker {
            // the marker bit is set on the last packet of an access unit
            return Ok(Vec::new());
        }

        let timestamp = self
            .rollover_state
            .timestamp(packet.packet.header.timestamp);
        let new_chunk = EncodedChunk {
            data: mem::take(&mut self.buffer).concat().into(),
            pts: Duration::from_secs_f64(timestamp as f64 / 90000.0),
            dts: None,
            is_keyframe: IsKeyframe::Unknown,
            kind: self.kind,
        };
        Ok(vec![new_chunk])
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DepayloaderInitError {
    #[error(transparent)]
    AacDepayloaderNewError(#[from] AacDepayloaderNewError),
}

#[derive(Default)]
pub struct RolloverState {
    previous_timestamp: Option<u32>,
    rollover_count: usize,
}

impl RolloverState {
    fn timestamp(&mut self, current_timestamp: u32) -> u64 {
        let Some(previous_timestamp) = self.previous_timestamp else {
            self.previous_timestamp = Some(current_timestamp);
            return current_timestamp as u64;
        };

        let timestamp_diff = u32::abs_diff(previous_timestamp, current_timestamp);
        if timestamp_diff >= u32::MAX / 2 {
            if previous_timestamp > current_timestamp {
                self.rollover_count += 1;
            } else {
                // We received a packet from before the rollover, so we need to decrement the count
                self.rollover_count = self.rollover_count.saturating_sub(1);
            }
        }

        self.previous_timestamp = Some(current_timestamp);

        (self.rollover_count as u64) * (u32::MAX as u64 + 1) + current_timestamp as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_rollover() {
        let mut rollover_state = RolloverState::default();

        let current_timestamp = 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            current_timestamp as u64
        );

        let current_timestamp = u32::MAX / 2 + 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            current_timestamp as u64
        );

        let current_timestamp = 0;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX);
        let current_timestamp = 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            2 * (u32::MAX as u64 + 1) + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(1);
        let current_timestamp = u32::MAX;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX);
        let current_timestamp = u32::MAX - 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX - 1);
        let current_timestamp = u32::MAX;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );
    }
}
