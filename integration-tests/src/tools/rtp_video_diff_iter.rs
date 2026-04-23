//! Pairs decoded video frames from two RTP dumps, decoding lazily.
//!
//! [`RtpVideoDiffIter::from_rtp_dumps`] reads two `.rtp` dumps, filters
//! to the H.264 video payload type, and yields pairs of decoded
//! frames as the iterator is advanced. Decoding only runs as far as
//! necessary to satisfy each step — useful for huge dumps where the
//! caller may stop after a handful of frames.
//!
//! ## Pairing strategy
//!
//! Each side has an independent playhead. On every step the playhead
//! whose *next* frame has the earlier presentation timestamp advances
//! by one; the other side's playhead stays put. The yielded pair is
//! always `(left_at_playhead, right_at_playhead)`.
//!
//! As a consequence, when both dumps share a framerate the iterator
//! tends to alternate sides — each step changes only one frame —
//! which lines up well with frame-by-frame visual diffing. When the
//! framerates differ, the faster side advances proportionally more
//! often.

use std::{collections::VecDeque, path::Path};

use anyhow::{Context, Result};
use bytes::Bytes;
use smelter_render::Frame;
use webrtc::rtp;

use crate::{unmarshal_packets, video_decoder::VideoDecoder};

/// RTP payload type smelter uses for H.264 video.
const VIDEO_PAYLOAD_TYPE: u8 = 96;

/// One step of the pairing iterator. A side is `None` only when that
/// dump has no remaining frames at all — once a side is exhausted it
/// stays exhausted.
#[derive(Debug, Clone)]
pub struct FramePair {
    pub left: Option<Frame>,
    pub right: Option<Frame>,
}

/// Iterator over paired decoded frames from two RTP dumps. Construct
/// via [`RtpVideoDiffIter::from_rtp_dumps`] for the on-disk path, or
/// [`RtpVideoDiffIter::from_frames`] for tests.
///
/// `Item = Result<FramePair>` because decoding happens lazily during
/// iteration and can fail at any point. Once an error is yielded the
/// iterator fuses (returns `None` thereafter).
pub struct RtpVideoDiffIter {
    left: LazyFrameStream,
    right: LazyFrameStream,
    state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    NotStarted,
    Active,
    Done,
}

impl RtpVideoDiffIter {
    /// Read both dumps from disk, but do not decode anything yet.
    /// Decoding runs lazily as the iterator is advanced.
    pub fn from_rtp_dumps(left: &Path, right: &Path) -> Result<Self> {
        Ok(Self {
            left: LazyFrameStream::from_dump_path(left)?,
            right: LazyFrameStream::from_dump_path(right)?,
            state: State::NotStarted,
        })
    }

    /// Build directly from already-decoded frames. Useful for tests.
    /// Inputs must be sorted by `pts` ascending.
    pub fn from_frames(left: Vec<Frame>, right: Vec<Frame>) -> Self {
        Self {
            left: LazyFrameStream::from_frames(left),
            right: LazyFrameStream::from_frames(right),
            state: State::NotStarted,
        }
    }
}

impl Iterator for RtpVideoDiffIter {
    type Item = Result<FramePair>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.step();
        match result {
            Ok(None) => None,
            Ok(Some(pair)) => Some(Ok(pair)),
            Err(e) => {
                self.state = State::Done;
                Some(Err(e))
            }
        }
    }
}

impl RtpVideoDiffIter {
    fn step(&mut self) -> Result<Option<FramePair>> {
        match self.state {
            State::Done => Ok(None),
            State::NotStarted => {
                self.left.advance()?;
                self.right.advance()?;
                if self.left.current().is_none() && self.right.current().is_none() {
                    self.state = State::Done;
                    return Ok(None);
                }
                self.state = State::Active;
                Ok(Some(self.pair()))
            }
            State::Active => {
                let next_l_pts = self.left.peek_next()?.map(|f| f.pts);
                let next_r_pts = self.right.peek_next()?.map(|f| f.pts);
                match (next_l_pts, next_r_pts) {
                    (None, None) => {
                        self.state = State::Done;
                        return Ok(None);
                    }
                    (Some(_), None) => self.left.advance()?,
                    (None, Some(_)) => self.right.advance()?,
                    (Some(lpts), Some(rpts)) => {
                        // Tie → advance left, arbitrarily but deterministically.
                        if lpts <= rpts {
                            self.left.advance()?;
                        } else {
                            self.right.advance()?;
                        }
                    }
                }
                Ok(Some(self.pair()))
            }
        }
    }

    fn pair(&self) -> FramePair {
        FramePair {
            left: self.left.current().cloned(),
            right: self.right.current().cloned(),
        }
    }
}

/// One side's lazy decode pipeline: a queue of pending RTP packets, a
/// shared decoder, a small buffer of frames already produced but not
/// yet consumed, and the frame currently shown on this side.
struct LazyFrameStream {
    decoder: Option<VideoDecoder>,
    packets: std::vec::IntoIter<rtp::packet::Packet>,
    pending: VecDeque<Frame>,
    drained: bool,
    current: Option<Frame>,
}

impl LazyFrameStream {
    fn from_dump_path(path: &Path) -> Result<Self> {
        let bytes = Bytes::from(
            std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?,
        );
        let packets = unmarshal_packets(&bytes)
            .with_context(|| format!("Failed to parse RTP dump {}", path.display()))?
            .into_iter()
            .filter(|p| p.header.payload_type == VIDEO_PAYLOAD_TYPE)
            .collect::<Vec<_>>();
        let decoder = VideoDecoder::new().with_context(|| {
            format!("Failed to initialize H.264 decoder for {}", path.display())
        })?;
        Ok(Self {
            decoder: Some(decoder),
            packets: packets.into_iter(),
            pending: VecDeque::new(),
            drained: false,
            current: None,
        })
    }

    fn from_frames(frames: Vec<Frame>) -> Self {
        Self {
            decoder: None,
            packets: Vec::new().into_iter(),
            pending: frames.into(),
            drained: true,
            current: None,
        }
    }

    fn current(&self) -> Option<&Frame> {
        self.current.as_ref()
    }

    /// Returns the next-to-be-shown frame without consuming it.
    /// Decodes more packets as needed.
    fn peek_next(&mut self) -> Result<Option<&Frame>> {
        self.refill()?;
        Ok(self.pending.front())
    }

    /// Promotes the next decoded frame (if any) to `current`.
    fn advance(&mut self) -> Result<()> {
        self.refill()?;
        if let Some(frame) = self.pending.pop_front() {
            self.current = Some(frame);
        }
        Ok(())
    }

    /// Pump packets through the decoder until at least one frame is
    /// pending or the input is fully drained.
    fn refill(&mut self) -> Result<()> {
        while self.pending.is_empty() && !self.drained {
            let Some(decoder) = self.decoder.as_mut() else {
                self.drained = true;
                return Ok(());
            };
            match self.packets.next() {
                Some(packet) => {
                    decoder.decode(packet)?;
                    for frame in decoder.drain_frames()? {
                        self.pending.push_back(frame);
                    }
                }
                None => {
                    // No more input packets: pull whatever the
                    // decoder has buffered, then mark drained.
                    for frame in decoder.drain_frames()? {
                        self.pending.push_back(frame);
                    }
                    self.drained = true;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smelter_render::{FrameData, Resolution, YuvPlanes};
    use std::time::Duration;

    fn frame(pts_ms: u64) -> Frame {
        Frame {
            data: FrameData::PlanarYuv420(YuvPlanes {
                y_plane: Bytes::new(),
                u_plane: Bytes::new(),
                v_plane: Bytes::new(),
            }),
            resolution: Resolution {
                width: 16,
                height: 16,
            },
            pts: Duration::from_millis(pts_ms),
        }
    }

    fn pts_pair(p: &FramePair) -> (Option<u64>, Option<u64>) {
        (
            p.left.as_ref().map(|f| f.pts.as_millis() as u64),
            p.right.as_ref().map(|f| f.pts.as_millis() as u64),
        )
    }

    fn collect_pairs(it: RtpVideoDiffIter) -> Vec<(Option<u64>, Option<u64>)> {
        it.map(|r| pts_pair(&r.unwrap())).collect()
    }

    #[test]
    fn equal_framerate_alternates_sides() {
        let left = vec![frame(0), frame(33), frame(66), frame(99)];
        let right = vec![frame(5), frame(38), frame(71)];
        assert_eq!(
            collect_pairs(RtpVideoDiffIter::from_frames(left, right)),
            vec![
                (Some(0), Some(5)),
                (Some(33), Some(5)),
                (Some(33), Some(38)),
                (Some(66), Some(38)),
                (Some(66), Some(71)),
                (Some(99), Some(71)),
            ],
        );
    }

    #[test]
    fn faster_side_advances_more_often() {
        let left = vec![frame(0), frame(33), frame(66)];
        let right = vec![frame(0), frame(16), frame(33), frame(50), frame(66)];
        let left_len = left.len();
        let right_len = right.len();
        let pairs = collect_pairs(RtpVideoDiffIter::from_frames(left, right));

        assert_eq!(pairs.first(), Some(&(Some(0), Some(0))));
        assert_eq!(pairs.last(), Some(&(Some(66), Some(66))));
        assert_eq!(pairs.len(), left_len + right_len - 1);
    }

    #[test]
    fn one_side_empty() {
        let pairs = collect_pairs(RtpVideoDiffIter::from_frames(
            vec![frame(0), frame(33)],
            vec![],
        ));
        assert_eq!(pairs, vec![(Some(0), None), (Some(33), None)]);
    }

    #[test]
    fn both_empty() {
        let mut it = RtpVideoDiffIter::from_frames(vec![], vec![]);
        assert!(it.next().is_none());
    }
}
