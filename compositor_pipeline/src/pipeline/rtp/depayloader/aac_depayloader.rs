use std::{io::Read, time::Duration};

use bytes::{Buf, BytesMut};
use tracing::trace;

use crate::pipeline::rtp::{
    depayloader::{AacAudioSpecificConfig, Depayloader, DepayloadingError},
    RtpPacket,
};
use crate::prelude::*;

pub struct AacDepayloader {
    mode: RtpAacDepayloaderMode,
    asc: AacAudioSpecificConfig,
}

impl AacDepayloader {
    pub(super) fn new(mode: RtpAacDepayloaderMode, asc: AacAudioSpecificConfig) -> Self {
        Self { mode, asc }
    }
}

impl Depayloader for AacDepayloader {
    /// Related spec:
    ///  - [RFC 3640, section 3.2. RTP Payload Structure](https://datatracker.ietf.org/doc/html/rfc3640#section-3.2)
    ///  - [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
    ///  - [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
    fn depayload(
        &mut self,
        packet: RtpPacket,
    ) -> Result<Vec<EncodedInputChunk>, DepayloadingError> {
        let mut reader = std::io::Cursor::new(packet.packet.payload);

        if reader.remaining() < 2 {
            return Err(AacDepayloadingError::PacketTooShort.into());
        }

        let headers_len = reader.get_u16() / 8;
        if reader.remaining() < headers_len as usize {
            return Err(AacDepayloadingError::PacketTooShort.into());
        }

        let header_len = header_len_in_bytes(self.mode);
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
                size: h >> index_len_in_bits(self.mode),
                index: (h & (u16::MAX >> size_len_in_bits(self.mode))) as u8,
            })
            .collect::<Vec<_>>();

        if headers.iter().any(|h| h.index != 0) {
            return Err(AacDepayloadingError::InterleavingNotSupported.into());
        }

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

            let pts = packet.timestamp + frame_duration * (i as u32);

            let chunk = EncodedInputChunk {
                pts,
                data: payload,
                dts: None,
                kind: MediaKind::Audio(AudioCodec::Aac),
            };
            trace!(?chunk, "RTP depayloader produced new chunk");
            chunks.push(chunk);
        }

        Ok(chunks)
    }
}

fn size_len_in_bits(mode: RtpAacDepayloaderMode) -> usize {
    match mode {
        RtpAacDepayloaderMode::LowBitrate => 6,
        RtpAacDepayloaderMode::HighBitrate => 13,
    }
}

fn index_len_in_bits(mode: RtpAacDepayloaderMode) -> usize {
    match mode {
        RtpAacDepayloaderMode::LowBitrate => 2,
        RtpAacDepayloaderMode::HighBitrate => 3,
    }
}

fn header_len_in_bytes(mode: RtpAacDepayloaderMode) -> usize {
    match mode {
        RtpAacDepayloaderMode::LowBitrate => 1,
        RtpAacDepayloaderMode::HighBitrate => 2,
    }
}
