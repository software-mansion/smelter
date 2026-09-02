use std::time::Duration;

use crate::pipeline::{decoder::EncodedInputEvent, rtp::RtpInputEvent};

use crate::prelude::*;

// Trait used to estimate duration the item
pub trait TimedValue {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)>;
}

impl TimedValue for RtpInputEvent {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        match self {
            RtpInputEvent::Packet(packet) => Some((
                packet.timestamp - Duration::from_millis(10),
                packet.timestamp + Duration::from_millis(10),
            )),
            RtpInputEvent::LostPacket => None,
        }
    }
}

impl TimedValue for Frame {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        Some((
            self.pts - Duration::from_millis(10),
            self.pts + Duration::from_millis(10),
        ))
    }
}

impl TimedValue for EncodedInputChunk {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        // dts should be monotonic, so better to estimate duration
        // of the set of chunks, but some chunks might be missing
        // dts and pts might be in a very different reference frame
        Some((
            self.pts - Duration::from_millis(10),
            self.pts + Duration::from_millis(10),
        ))
    }
}

impl TimedValue for EncodedInputEvent {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        match self {
            EncodedInputEvent::Chunk(chunk) => chunk.timestamp_range(),
            // markers do not extend the buffered range
            EncodedInputEvent::LostData
            | EncodedInputEvent::AuDelimiter
            | EncodedInputEvent::Discontinuity => None,
        }
    }
}

impl TimedValue for InputAudioSamples {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        Some(self.pts_range())
    }
}

impl<T: TimedValue> TimedValue for PipelineEvent<T> {
    fn timestamp_range(&self) -> Option<(Timestamp, Timestamp)> {
        match self {
            PipelineEvent::Data(inner) => inner.timestamp_range(),
            PipelineEvent::EOS => None,
        }
    }
}
