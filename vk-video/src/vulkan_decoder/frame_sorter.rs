use std::collections::BinaryHeap;

use crate::Frame;

use super::DecodeResult;

impl<T> PartialEq for DecodeResult<T> {
    fn eq(&self, other: &Self) -> bool {
        self.pic_order_cnt.eq(&other.pic_order_cnt)
    }
}

impl<T> Eq for DecodeResult<T> {}

impl<T> PartialOrd for DecodeResult<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for DecodeResult<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pic_order_cnt.cmp(&other.pic_order_cnt).reverse()
    }
}

pub(crate) struct FrameSorter<T> {
    frames: BinaryHeap<DecodeResult<T>>,
}

impl<T> FrameSorter<T> {
    pub(crate) fn new() -> Self {
        Self {
            frames: BinaryHeap::new(),
        }
    }

    pub(crate) fn put(&mut self, frame: DecodeResult<T>) -> Vec<Frame<T>> {
        let max_num_reorder_frames = frame.max_num_reorder_frames as usize;
        let is_idr = frame.is_idr;
        let mut result = Vec::new();

        if is_idr {
            while !self.frames.is_empty() {
                let frame = self.frames.pop().unwrap();

                result.push(Frame {
                    data: frame.frame,
                    pts: frame.pts,
                });
            }

            result.push(Frame {
                data: frame.frame,
                pts: frame.pts,
            });
        } else {
            self.frames.push(frame);

            while self.frames.len() > max_num_reorder_frames {
                let frame = self.frames.pop().unwrap();

                result.push(Frame {
                    data: frame.frame,
                    pts: frame.pts,
                });
            }
        }

        result
    }

    pub(crate) fn flush(&mut self) -> Vec<Frame<T>> {
        let mut result = Vec::with_capacity(self.frames.len());

        while !self.frames.is_empty() {
            let frame = self.frames.pop().unwrap();
            result.push(Frame {
                data: frame.frame,
                pts: frame.pts,
            });
        }

        result
    }
}
