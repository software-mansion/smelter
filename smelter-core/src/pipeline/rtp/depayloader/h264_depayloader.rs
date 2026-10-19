use bytes::Bytes;
use tracing::trace;
use webrtc::rtp::{
    self,
    codecs::h264::{
        FU_END_BITMASK, FU_START_BITMASK, FUA_NALU_TYPE, H264Packet, NALU_TYPE_BITMASK,
        STAPA_HEADER_SIZE, STAPA_NALU_TYPE,
    },
    packetizer::Depacketizer,
};

use crate::pipeline::{
    decoder::EncodedInputEvent,
    rtp::{
        RtpPacket,
        depayloader::{Depayloader, DepayloadingError},
    },
};

use crate::prelude::*;

/// The webrtc-rs depacketizer reassembles FU-A without checking the start bit and keeps a partial
/// NAL unit after a lost end fragment, so a lost packet turns into a corrupted NAL unit. Here a
/// fragmented NAL unit that lost any of its packets is dropped instead.
#[derive(Default)]
pub(super) struct H264Depayloader {
    depacketizer: H264Packet,
    /// FU-A NAL unit is being reassembled, and none of its fragments were lost.
    in_fragmented_nalu: bool,
}

impl H264Depayloader {
    fn reset(&mut self) {
        self.depacketizer = H264Packet::default();
        self.in_fragmented_nalu = false;
    }

    fn depacketize(&mut self, payload: &Bytes) -> Result<Bytes, DepayloadingError> {
        // Padding-only packets (e.g. bandwidth probing) carry no data.
        if payload.is_empty() {
            return Ok(Bytes::new());
        }

        let fu_header = match payload.as_ref() {
            [indicator, fu_header, ..] if indicator & NALU_TYPE_BITMASK == FUA_NALU_TYPE => {
                Some(*fu_header)
            }
            _ => None,
        };

        match fu_header {
            // Previous fragmented NAL unit is missing its end.
            None if self.in_fragmented_nalu => self.reset(),
            None => (),
            // Discard a partial NAL unit, so it is not prepended to this one.
            Some(fu_header) if fu_header & FU_START_BITMASK != 0 => self.reset(),
            Some(_) if !self.in_fragmented_nalu => {
                trace!("Dropping FU-A fragment of an incomplete NAL unit");
                return Ok(Bytes::new());
            }
            Some(_) => (),
        }

        if is_malformed_stap_a(payload) {
            return Err(rtp::Error::ErrShortPacket.into());
        }

        let data = match self.depacketizer.depacketize(payload) {
            Ok(data) => data,
            Err(err) => {
                self.reset();
                return Err(err.into());
            }
        };

        // A STAP-A entry of size 0, or a start code inside a malformed NAL unit, leaves a start code
        // with no NAL unit after it (webrtc-rs does not reject either), and the H264 parser panics
        // on such an empty NAL unit.
        let has_empty_nalu = memchr::memmem::find_iter(&data, &[0, 0, 1]).any(|i| {
            let rest = &data[i + 3..];
            rest.is_empty() || rest.starts_with(&[0, 0, 1]) || rest.starts_with(&[0, 0, 0, 1])
        });
        if has_empty_nalu {
            return Err(rtp::Error::ErrShortPacket.into());
        }
        self.in_fragmented_nalu = fu_header.is_some_and(|h| h & FU_END_BITMASK == 0);
        Ok(data)
    }
}

/// STAP-A entries don't fill the payload exactly. webrtc-rs panics on a single trailing byte.
fn is_malformed_stap_a(payload: &[u8]) -> bool {
    if payload[0] & NALU_TYPE_BITMASK != STAPA_NALU_TYPE {
        return false;
    }
    let mut rest = &payload[STAPA_HEADER_SIZE..];
    while let [hi, lo, tail @ ..] = rest {
        let size = usize::from(u16::from_be_bytes([*hi, *lo]));
        match tail.get(size..) {
            Some(next) => rest = next,
            None => return true,
        }
    }
    !rest.is_empty()
}

impl Depayloader for H264Depayloader {
    fn depayload(
        &mut self,
        packet: RtpPacket,
    ) -> Result<Vec<EncodedInputEvent>, DepayloadingError> {
        trace!(?packet, "RTP H264 depayloader received new packet");
        let data = self.depacketize(&packet.packet.payload)?;

        let mut events = Vec::new();
        if !data.is_empty() {
            events.push(EncodedInputEvent::Chunk(EncodedInputChunk {
                data,
                pts: packet.timestamp,
                dts: None,
                kind: MediaKind::Video(VideoCodec::H264),
                decode_only: false,
            }));
        }
        // Marker is the last packet of an access unit, even if its data was dropped.
        if packet.packet.header.marker {
            events.push(EncodedInputEvent::AuDelimiter);
        }
        trace!(?events, "RTP H264 depayloader produced events");
        Ok(events)
    }

    fn on_lost_packet(&mut self) {
        self.reset();
    }
}
