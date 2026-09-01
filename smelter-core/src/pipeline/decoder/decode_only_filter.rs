use std::time::Duration;

use crate::pipeline::decoder::EncodedInputEvent;

/// Tracks which decoded frames/samples came from decode-only chunks.
///
/// Decoders buffer and reorder, so frames from a decode-only chunk may come out
/// of a later `decode` call. Track pts ranges instead:
/// - Open range on first decode_only chunk
/// - Close range on first !decode_only chunk
/// - Drop only output inside a range
///
/// With non-monotonic PTS (B-frames) a chunk of the boundary GOP can arrive
/// after the range was opened/closed with a smaller PTS, so bounds move down
/// after the fact.
#[derive(Default)]
pub(crate) struct DecodeOnlyFilter {
    /// Pts ranges `[start, end)` that must not be presented; an open range
    /// (`end` is `None`) has not seen a presentable chunk yet.
    ranges: Vec<(Duration, Option<Duration>)>,
    /// Ranges describe a timeline that ended; the next chunk starts over.
    reset_on_next_chunk: bool,
}

impl DecodeOnlyFilter {
    /// Call before passing the event to the decoder.
    pub fn on_event(&mut self, event: &EncodedInputEvent) {
        match event {
            EncodedInputEvent::Chunk(chunk) => {
                if self.reset_on_next_chunk {
                    self.ranges.clear();
                    self.reset_on_next_chunk = false;
                }
                match (chunk.decode_only, self.ranges.last_mut()) {
                    // No ranges so always start new
                    (true, None) => self.ranges.push((chunk.pts, None)),

                    // Last range is closed, and PTS is outside of that range
                    (true, Some((_start, Some(end)))) if chunk.pts >= *end => {
                        self.ranges.push((chunk.pts, None))
                    }

                    // Either inside last range or even before
                    (true, Some((start, _end))) => {
                        *start = Duration::min(*start, chunk.pts);
                    }

                    // modify existing range even if closed if current PTS is smaller
                    (false, Some((_, end))) => {
                        *end = Some(Duration::min(end.unwrap_or(Duration::MAX), chunk.pts));
                    }

                    (false, None) => {}
                }
            }
            EncodedInputEvent::Discontinuity => {
                // Everything still buffered belongs to the old timeline and
                // nothing presentable follows it there.
                if let Some((_, end @ None)) = self.ranges.last_mut() {
                    *end = Some(Duration::MAX);
                }
                self.reset_on_next_chunk = true;
            }
            EncodedInputEvent::LostData | EncodedInputEvent::AuDelimiter => {}
        }
    }

    /// Call for every frame/sample batch the decoder returns, in the order
    /// it was returned.
    pub fn should_drop(&mut self, pts: Duration) -> bool {
        // Output leaves the decoder in presentation order, so closed ranges
        // ending at or before this pts can never match again.
        self.ranges
            .retain(|(_, end)| end.is_none_or(|end| end > pts));
        self.ranges
            .iter()
            .any(|(start, end)| *start <= pts && end.is_none_or(|end| pts < end))
    }
}
