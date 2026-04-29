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
//! Each side has an independent playhead. On every step we look at
//! three candidate next pairs — advance left only, advance right
//! only, or advance both — and pick the one whose `|left.pts -
//! right.pts|` is smallest. Ties prefer "advance both" so the
//! iterator doesn't stall on one side. The yielded pair is always
//! `(left_at_playhead, right_at_playhead)`.
//!
//! Effect: when both dumps share a framerate, both sides advance
//! together and the iterator produces one pair per pair of input
//! frames (rather than zig-zagging through each side independently).
//! When framerates differ, the faster side advances on its own until
//! the slow side catches up, at which point both sides advance.
//!
//! Example — left `[1, 6, 11]`, right `[2, 7, 12]`. Only three pairs:
//! `(1, 2)`, `(6, 7)`, `(11, 12)`.

use std::{collections::VecDeque, path::Path};

use anyhow::{Context, Result};
use bytes::Bytes;
use smelter_render::Frame;
use webrtc::rtp;

use crate::{unmarshal_packets, video_decoder::VideoDecoder};

/// RTP payload type smelter uses for H.264 video.
pub(crate) const VIDEO_PAYLOAD_TYPE: u8 = 96;

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
    /// Decoding runs lazily as the iterator is advanced. Either side
    /// pointing at a non-existent file is treated as an empty stream
    /// so the inspector can still surface the side that does exist —
    /// useful when there is no committed snapshot to diff against yet.
    pub fn from_rtp_dumps(left: &Path, right: &Path) -> Result<Self> {
        Ok(Self {
            left: LazyFrameStream::from_dump_path_or_empty(left)?,
            right: LazyFrameStream::from_dump_path_or_empty(right)?,
            state: State::NotStarted,
        })
    }

    /// Same as [`Self::from_rtp_dumps`] but consumes already-loaded
    /// dumps instead of paths. Used by the test harness, which has the
    /// `actual` dump in memory and only the `expected` on disk.
    pub fn from_bytes(left: &Bytes, right: &Bytes) -> Result<Self> {
        Ok(Self {
            left: LazyFrameStream::from_bytes(left)?,
            right: LazyFrameStream::from_bytes(right)?,
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
                    (Some(lp), Some(rp)) => {
                        // Both currents are guaranteed Some here:
                        // peek_next can only be Some after the first
                        // advance, and `NotStarted` advances both.
                        let cur_l = self.left.current().expect("active state").pts;
                        let cur_r = self.right.current().expect("active state").pts;
                        let dist_left = pts_distance(lp, cur_r);
                        let dist_right = pts_distance(cur_l, rp);
                        let dist_both = pts_distance(lp, rp);
                        let min = dist_both.min(dist_left).min(dist_right);
                        // Prefer "advance both" on ties so the
                        // iterator doesn't stall on one side.
                        if dist_both == min {
                            self.left.advance()?;
                            self.right.advance()?;
                        } else if dist_left <= dist_right {
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

fn pts_distance(a: std::time::Duration, b: std::time::Duration) -> std::time::Duration {
    if a >= b { a - b } else { b - a }
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
    /// Same as [`Self::from_dump_path`] but yields an empty (already
    /// drained) stream instead of erroring when `path` doesn't exist.
    fn from_dump_path_or_empty(path: &Path) -> Result<Self> {
        if !path.exists() {
            tracing::warn!(
                "rtp_video_diff_iter: dump {} not found, treating as empty",
                path.display()
            );
            return Ok(Self::from_frames(Vec::new()));
        }
        Self::from_dump_path(path)
    }

    fn from_dump_path(path: &Path) -> Result<Self> {
        let bytes = Bytes::from(
            std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?,
        );
        Self::from_bytes(&bytes)
            .with_context(|| format!("Failed to build frame stream from {}", path.display()))
    }

    fn from_bytes(bytes: &Bytes) -> Result<Self> {
        let packets = unmarshal_packets(bytes)
            .context("Failed to parse RTP dump")?
            .into_iter()
            .filter(|p| p.header.payload_type == VIDEO_PAYLOAD_TYPE)
            .collect::<Vec<_>>();
        let decoder = VideoDecoder::new().context("Failed to initialize H.264 decoder")?;
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
    fn equal_framerate_advances_both_sides() {
        // Both streams run at the same cadence with a fixed offset; the
        // iterator should pair frame-for-frame instead of zig-zagging.
        let left = vec![frame(1), frame(6), frame(11)];
        let right = vec![frame(2), frame(7), frame(12)];
        assert_eq!(
            collect_pairs(RtpVideoDiffIter::from_frames(left, right)),
            vec![
                (Some(1), Some(2)),
                (Some(6), Some(7)),
                (Some(11), Some(12)),
            ],
        );
    }

    #[test]
    fn equal_framerate_with_extra_left_frame() {
        // Left has one frame past the right's tail — that frame pairs
        // with the last right frame (only candidate).
        let left = vec![frame(0), frame(33), frame(66), frame(99)];
        let right = vec![frame(5), frame(38), frame(71)];
        assert_eq!(
            collect_pairs(RtpVideoDiffIter::from_frames(left, right)),
            vec![
                (Some(0), Some(5)),
                (Some(33), Some(38)),
                (Some(66), Some(71)),
                (Some(99), Some(71)),
            ],
        );
    }

    #[test]
    fn faster_side_advances_more_often() {
        // Right runs at twice the framerate of left. Where the two
        // align (0, 33, 66) we advance both; in between, only right
        // advances.
        let left = vec![frame(0), frame(33), frame(66)];
        let right = vec![frame(0), frame(16), frame(33), frame(50), frame(66)];
        assert_eq!(
            collect_pairs(RtpVideoDiffIter::from_frames(left, right)),
            vec![
                (Some(0), Some(0)),
                (Some(0), Some(16)),
                (Some(33), Some(33)),
                (Some(66), Some(50)),
                (Some(66), Some(66)),
            ],
        );
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
