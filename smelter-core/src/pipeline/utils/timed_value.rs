use std::time::Duration;

use smelter_render::Frame;

use crate::pipeline::rtp::RtpInputEvent;

use crate::prelude::*;

// Trait used to estimate duration the item
pub trait TimedValue {
    fn timestamp_range(&self) -> Option<(Duration, Duration)>;
}

impl TimedValue for RtpInputEvent {
    fn timestamp_range(&self) -> Option<(Duration, Duration)> {
        match self {
            RtpInputEvent::Packet(packet) => Some((
                packet.timestamp.saturating_sub(Duration::from_millis(10)),
                packet.timestamp + Duration::from_millis(10),
            )),
            RtpInputEvent::LostPacket => None,
        }
    }
}

impl TimedValue for Frame {
    fn timestamp_range(&self) -> Option<(Duration, Duration)> {
        Some((
            self.pts.saturating_sub(Duration::from_millis(10)),
            self.pts + Duration::from_millis(10),
        ))
    }
}

impl TimedValue for EncodedInputChunk {
    fn timestamp_range(&self) -> Option<(Duration, Duration)> {
        // dts should be monotonic, so better to estimate duration
        // of the set of chunks, but some chunks might be missing
        // dts and pts might be in a very different reference frame
        Some((
            self.pts.saturating_sub(Duration::from_millis(10)),
            self.pts + Duration::from_millis(10),
        ))
    }
}

impl TimedValue for InputAudioSamples {
    fn timestamp_range(&self) -> Option<(Duration, Duration)> {
        Some(self.pts_range())
    }
}

impl<T: TimedValue> TimedValue for PipelineEvent<T> {
    fn timestamp_range(&self) -> Option<(Duration, Duration)> {
        match self {
            PipelineEvent::Data(inner) => inner.timestamp_range(),
            PipelineEvent::EOS => None,
        }
    }
}
