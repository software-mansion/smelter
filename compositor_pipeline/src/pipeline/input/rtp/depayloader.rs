use std::{mem, time::Duration};

use bytes::Bytes;
use log::error;
use rtp::{
    codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet},
    packetizer::Depacketizer,
};

use crate::pipeline::{
    decoder::{self, AacDecoderOptions},
    rtp::{AUDIO_PAYLOAD_TYPE, VIDEO_PAYLOAD_TYPE},
    types::{AudioCodec, EncodedChunk, EncodedChunkKind, IsKeyframe, VideoCodec},
    VideoDecoder,
};

use self::aac::AacDepayloaderNewError;

use super::{DepayloadingError, RtpStream};

pub use aac::{AacDepayloader, AacDepayloadingError};

mod aac;

#[derive(Debug, thiserror::Error)]
pub enum DepayloaderNewError {
    #[error(transparent)]
    Audio(#[from] AudioDepayloaderNewError),
}

pub(crate) struct Depayloader {
    /// (Depayloader, payload type)
    pub video: Option<VideoDepayloader>,
    pub audio: Option<AudioDepayloader>,
}

impl Depayloader {
    pub fn new(stream: &RtpStream) -> Result<Self, DepayloaderNewError> {
        let video = stream
            .video
            .as_ref()
            .map(|video| VideoDepayloader::new(&video.options));

        let audio = stream
            .audio
            .as_ref()
            .map(|audio| AudioDepayloader::new(&audio.options))
            .transpose()?;

        Ok(Self { video, audio })
    }

    pub fn depayload(
        &mut self,
        packet: rtp::packet::Packet,
    ) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        match packet.header.payload_type {
            VIDEO_PAYLOAD_TYPE => match self.video.as_mut() {
                Some(video_depayloader) => video_depayloader.depayload(packet),
                None => Err(DepayloadingError::BadPayloadType(
                    packet.header.payload_type,
                )),
            },
            AUDIO_PAYLOAD_TYPE => match self.audio.as_mut() {
                Some(audio_depayloader) => audio_depayloader.depayload(packet),
                None => Err(DepayloadingError::BadPayloadType(
                    packet.header.payload_type,
                )),
            },
            other => Err(DepayloadingError::BadPayloadType(other)),
        }
    }
}

pub enum VideoDepayloader {
    H264 {
        depayloader: H264Packet,
        buffer: Vec<Bytes>,
        rollover_state: RolloverState,
    },
    VP8 {
        depayloader: Vp8Packet,
        buffer: Vec<Bytes>,
        rollover_state: RolloverState,
    },
    VP9 {
        depayloader: Vp9Packet,
        buffer: Vec<Bytes>,
        rollover_state: RolloverState,
    },
}

impl VideoDepayloader {
    pub fn new(options: &decoder::VideoDecoderOptions) -> Self {
        match options.decoder {
            VideoDecoder::FFmpegH264 => VideoDepayloader::H264 {
                depayloader: H264Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },
            VideoDecoder::FFmpegVp8 => VideoDepayloader::VP8 {
                depayloader: Vp8Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },
            VideoDecoder::FFmpegVp9 => VideoDepayloader::VP9 {
                depayloader: Vp9Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },

            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanVideoH264 => VideoDepayloader::H264 {
                depayloader: H264Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },
        }
    }

    fn depayload(
        &mut self,
        packet: rtp::packet::Packet,
    ) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        match self {
            VideoDepayloader::H264 {
                depayloader,
                buffer,
                rollover_state,
            } => {
                let kind = EncodedChunkKind::Video(VideoCodec::H264);
                let h264_chunk = depayloader.depacketize(&packet.payload)?;

                if h264_chunk.is_empty() {
                    return Ok(Vec::new());
                }

                buffer.push(h264_chunk);
                if !packet.header.marker {
                    // the marker bit is set on the last packet of an access unit
                    return Ok(Vec::new());
                }

                let timestamp = rollover_state.timestamp(packet.header.timestamp);
                let new_chunk = EncodedChunk {
                    data: mem::take(buffer).concat().into(),
                    pts: Duration::from_secs_f64(timestamp as f64 / 90000.0),
                    dts: None,
                    is_keyframe: IsKeyframe::Unknown,
                    kind,
                };

                Ok(vec![new_chunk])
            }
            VideoDepayloader::VP8 {
                depayloader,
                buffer,
                rollover_state,
            } => {
                let kind = EncodedChunkKind::Video(VideoCodec::VP8);
                let vp8_chunk = depayloader.depacketize(&packet.payload)?;

                if vp8_chunk.is_empty() {
                    return Ok(Vec::new());
                }

                buffer.push(vp8_chunk);
                if !packet.header.marker {
                    // the marker bit is set on the last packet of an access unit
                    return Ok(Vec::new());
                }

                let timestamp = rollover_state.timestamp(packet.header.timestamp);
                let new_chunk = EncodedChunk {
                    data: mem::take(buffer).concat().into(),
                    pts: Duration::from_secs_f64(timestamp as f64 / 90000.0),
                    dts: None,
                    is_keyframe: IsKeyframe::Unknown,
                    kind,
                };

                Ok(vec![new_chunk])
            }
            VideoDepayloader::VP9 {
                depayloader,
                buffer,
                rollover_state,
            } => {
                let kind = EncodedChunkKind::Video(VideoCodec::VP9);
                let vp9_chunk = depayloader.depacketize(&packet.payload)?;

                if vp9_chunk.is_empty() {
                    return Ok(Vec::new());
                }

                buffer.push(vp9_chunk);
                if !packet.header.marker {
                    // the marker bit is set on the last packet of an access unit
                    return Ok(Vec::new());
                }

                let timestamp = rollover_state.timestamp(packet.header.timestamp);
                let new_chunk = EncodedChunk {
                    data: mem::take(buffer).concat().into(),
                    pts: Duration::from_secs_f64(timestamp as f64 / 90000.0),
                    dts: None,
                    is_keyframe: IsKeyframe::Unknown,
                    kind,
                };

                Ok(vec![new_chunk])
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioDepayloaderNewError {
    #[error("Unsupported depayloader for provided decoder settings: {0:?}")]
    UnsupportedDepayloader(decoder::AudioDecoderOptions),

    #[error("No required depayloader settings were provided")]
    DepayloaderSettingsRequired,

    #[error(transparent)]
    AacDepayloaderNewError(#[from] AacDepayloaderNewError),
}

pub enum AudioDepayloader {
    Opus {
        depayloader: OpusPacket,
        rollover_state: RolloverState,
    },
    Aac(AacDepayloader),
}

impl AudioDepayloader {
    pub fn new(options: &decoder::AudioDecoderOptions) -> Result<Self, AudioDepayloaderNewError> {
        match options {
            decoder::AudioDecoderOptions::Opus(_) => Ok(AudioDepayloader::Opus {
                depayloader: OpusPacket,
                rollover_state: RolloverState::default(),
            }),
            decoder::AudioDecoderOptions::Aac(AacDecoderOptions {
                depayloader_mode,
                asc,
            }) => Ok(AudioDepayloader::Aac(AacDepayloader::new(
                depayloader_mode.ok_or(AudioDepayloaderNewError::DepayloaderSettingsRequired)?,
                asc.as_ref()
                    .ok_or(AudioDepayloaderNewError::DepayloaderSettingsRequired)?,
            )?)),
        }
    }

    fn depayload(
        &mut self,
        packet: rtp::packet::Packet,
    ) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        match self {
            AudioDepayloader::Opus {
                depayloader,
                rollover_state,
            } => {
                let kind = EncodedChunkKind::Audio(AudioCodec::Opus);
                let opus_packet = depayloader.depacketize(&packet.payload)?;

                if opus_packet.is_empty() {
                    return Ok(Vec::new());
                }

                let timestamp = rollover_state.timestamp(packet.header.timestamp);
                Ok(vec![EncodedChunk {
                    data: opus_packet,
                    pts: Duration::from_secs_f64(timestamp as f64 / 48000.0),
                    dts: None,
                    is_keyframe: IsKeyframe::NoKeyframes,
                    kind,
                }])
            }

            AudioDepayloader::Aac(aac) => Ok(aac.depayload(packet)?),
        }
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
