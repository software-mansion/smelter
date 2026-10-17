use bytes::Bytes;
use tracing::trace;
use webrtc::rtp::{
    codecs::h264::{
        FU_END_BITMASK, FU_START_BITMASK, FUA_NALU_TYPE, H264Packet, NALU_TYPE_BITMASK,
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

        let data = match self.depacketizer.depacketize(payload) {
            Ok(data) => data,
            Err(err) => {
                self.reset();
                return Err(err.into());
            }
        };
        self.in_fragmented_nalu = fu_header.is_some_and(|h| h & FU_END_BITMASK == 0);
        Ok(data)
    }
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
