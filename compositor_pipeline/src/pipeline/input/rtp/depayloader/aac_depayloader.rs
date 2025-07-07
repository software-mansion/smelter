use std::{io::Read, time::Duration};

use bytes::{Buf, BytesMut};

use crate::pipeline::{
    input::rtp::{
        depayloader::{AudioSpecificConfig, Depayloader},
        DepayloadingError, RtpAacDepayloaderMode,
    },
    output::rtp::RtpPacket,
    types::{EncodedChunk, EncodedChunkKind, IsKeyframe},
    AudioCodec,
};

use super::RolloverState;

#[derive(Debug, thiserror::Error)]
pub enum AacDepayloadingError {
    #[error("Packet too short")]
    PacketTooShort,

    #[error("Interleaving is not supported")]
    InterleavingNotSupported,
}

impl RtpAacDepayloaderMode {
    fn size_len_in_bits(&self) -> usize {
        match self {
            RtpAacDepayloaderMode::LowBitrate => 6,
            RtpAacDepayloaderMode::HighBitrate => 13,
        }
    }

    fn index_len_in_bits(&self) -> usize {
        match self {
            RtpAacDepayloaderMode::LowBitrate => 2,
            RtpAacDepayloaderMode::HighBitrate => 3,
        }
    }

    fn header_len_in_bytes(&self) -> usize {
        match self {
            RtpAacDepayloaderMode::LowBitrate => 1,
            RtpAacDepayloaderMode::HighBitrate => 2,
        }
    }
}

pub struct AacDepayloader {
    mode: RtpAacDepayloaderMode,
    asc: AudioSpecificConfig,
    rollover_state: RolloverState,
}

impl AacDepayloader {
    pub(super) fn new(mode: RtpAacDepayloaderMode, asc: AudioSpecificConfig) -> Self {
        Self {
            mode,
            asc,
            rollover_state: RolloverState::default(),
        }
    }
}

impl Depayloader for AacDepayloader {
    /// Related spec:
    ///  - [RFC 3640, section 3.2. RTP Payload Structure](https://datatracker.ietf.org/doc/html/rfc3640#section-3.2)
    ///  - [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
    ///  - [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        let mut reader = std::io::Cursor::new(packet.packet.payload);

        if reader.remaining() < 2 {
            return Err(AacDepayloadingError::PacketTooShort.into());
        }

        let headers_len = reader.get_u16() / 8;
        if reader.remaining() < headers_len as usize {
            return Err(AacDepayloadingError::PacketTooShort.into());
        }

        let header_len = self.mode.header_len_in_bytes();
        let header_count = headers_len as usize / header_len;
        let mut headers = Vec::new();

        for _ in 0..header_count {
            let mut header: u16 = 0;
            for _ in 0..header_len {
                header <<= 8;
                header |= reader.get_u8() as u16;
            }

            headers.push(header);
        }

        struct Header {
            index: u8,
            size: u16,
        }

        let headers = headers
            .into_iter()
            .map(|h| Header {
                size: h >> self.mode.index_len_in_bits(),
                index: (h & (u16::MAX >> self.mode.size_len_in_bits())) as u8,
            })
            .collect::<Vec<_>>();

        if headers.iter().any(|h| h.index != 0) {
            return Err(AacDepayloadingError::InterleavingNotSupported.into());
        }

        let packet_pts = self
            .rollover_state
            .timestamp(packet.packet.header.timestamp);
        let packet_pts = Duration::from_secs_f64(packet_pts as f64 / self.asc.sample_rate as f64);
        let frame_duration =
            Duration::from_secs_f64(self.asc.frame_length as f64 / self.asc.sample_rate as f64);
        let mut chunks = Vec::new();
        for (i, header) in headers.iter().enumerate() {
            if reader.remaining() < header.size.into() {
                return Err(AacDepayloadingError::PacketTooShort.into());
            }

            let mut payload = BytesMut::zeroed(header.size as usize);
            reader.read_exact(&mut payload).unwrap();
            let payload = payload.freeze();

            let pts = packet_pts + frame_duration * (i as u32);

            chunks.push(EncodedChunk {
                pts,
                data: payload,
                dts: None,
                is_keyframe: IsKeyframe::NoKeyframes,
                kind: EncodedChunkKind::Audio(AudioCodec::Aac),
            });
        }

        Ok(chunks)
    }
}
