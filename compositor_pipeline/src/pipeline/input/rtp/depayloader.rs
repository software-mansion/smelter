use std::mem;

use bytes::Bytes;
use compositor_render::error::ErrorStack;
use rtp::codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet};
use tracing::warn;

use crate::{
    pipeline::{
        input::rtp::RtpAacDepayloaderMode,
        output::rtp::RtpPacket,
        types::{AudioCodec, EncodedChunk, EncodedChunkKind, IsKeyframe, VideoCodec},
    },
    queue::PipelineEvent,
};

use super::DepayloadingError;

pub use aac_asc::AudioSpecificConfig;
pub use aac_depayloader::{AacDepayloader, AacDepayloadingError};

mod aac_asc;
mod aac_depayloader;

pub enum DepayloaderOptions {
    H264,
    Vp8,
    Vp9,
    Opus,
    Aac(RtpAacDepayloaderMode, AudioSpecificConfig),
}

pub fn new_depayloader(options: DepayloaderOptions) -> Box<dyn Depayloader> {
    match options {
        DepayloaderOptions::H264 => {
            BufferedDepayloader::<H264Packet>::new(EncodedChunkKind::Video(VideoCodec::H264))
        }
        DepayloaderOptions::Vp8 => {
            BufferedDepayloader::<Vp8Packet>::new(EncodedChunkKind::Video(VideoCodec::VP8))
        }
        DepayloaderOptions::Vp9 => {
            BufferedDepayloader::<Vp9Packet>::new(EncodedChunkKind::Video(VideoCodec::VP9))
        }
        DepayloaderOptions::Opus => {
            BufferedDepayloader::<OpusPacket>::new(EncodedChunkKind::Audio(AudioCodec::Opus))
        }
        DepayloaderOptions::Aac(mode, asc) => Box::new(AacDepayloader::new(mode, asc)),
    }
}

trait Depayloader {
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError>;
}

struct BufferedDepayloader<T: rtp::packetizer::Depacketizer + Default + 'static> {
    kind: EncodedChunkKind,
    buffer: Vec<Bytes>,
    depayloader: T,
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> BufferedDepayloader<T> {
    fn new(kind: EncodedChunkKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            buffer: Vec::new(),
            depayloader: T::default(),
        })
    }
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> Depayloader for BufferedDepayloader<T> {
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        let chunk = self.depayloader.depacketize(&packet.packet.payload)?;

        if chunk.is_empty() {
            return Ok(Vec::new());
        }

        self.buffer.push(chunk);
        if !packet.packet.header.marker {
            // the marker bit is set on the last packet of an access unit
            return Ok(Vec::new());
        }

        let new_chunk = EncodedChunk {
            data: mem::take(&mut self.buffer).concat().into(),
            pts: packet.timestamp,
            dts: None,
            is_keyframe: IsKeyframe::Unknown,
            kind: self.kind,
        };
        Ok(vec![new_chunk])
    }
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

pub(crate) struct DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    depayloader: Box<dyn Depayloader>,
    source: Source,
    eos_sent: bool,
}

impl<Source> DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    pub fn new(options: DepayloaderOptions, source: Source) -> Self {
        Self {
            depayloader: new_depayloader(options),
            source,
            eos_sent: false,
        }
    }
}

impl<Source> Iterator for DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    type Item = Vec<PipelineEvent<EncodedChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(packet)) => match self.depayloader.depayload(packet) {
                Ok(chunks) => Some(chunks.into_iter().map(|p| PipelineEvent::Data(p)).collect()),
                Err(err) => {
                    warn!("Depayloader error: {}", ErrorStack::new(&err).into_string());
                    Some(vec![])
                }
            },
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
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
