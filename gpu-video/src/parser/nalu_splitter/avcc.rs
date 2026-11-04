use bytes::{Buf, BufMut, BytesMut};

use super::to_output_nalu;
use crate::parser::h264::AvccPrefixLength;

#[derive(Debug, Default)]
enum State {
    #[default]
    ReadingLengthPrefix,
    AtLengthPrefixEnd {
        len: usize,
    },
    InNalu {
        len: usize,
        payload_start_pts: Option<u64>,
    },
}

#[derive(Debug)]
pub(crate) struct AvccSplitter {
    buffer: BytesMut,
    state: State,
    prefix_len: usize,
}

impl AvccSplitter {
    pub(super) fn new(prefix_len: AvccPrefixLength) -> Self {
        Self {
            buffer: Default::default(),
            state: Default::default(),
            prefix_len: prefix_len.size(),
        }
    }

    pub(super) fn push(
        &mut self,
        chunk: &[u8],
        chunk_pts: Option<u64>,
    ) -> Vec<(Vec<u8>, Option<u64>)> {
        if let State::AtLengthPrefixEnd { len } = self.state
            && !chunk.is_empty()
        {
            self.state = State::InNalu {
                len,
                payload_start_pts: chunk_pts,
            };
        }

        self.buffer.put(chunk);
        let mut result = Vec::new();

        loop {
            match self.state {
                State::ReadingLengthPrefix => {
                    if self.buffer.len() < self.prefix_len {
                        break;
                    }
                    let len = self.buffer.get_uint(self.prefix_len) as usize;
                    self.state = if self.buffer.is_empty() {
                        State::AtLengthPrefixEnd { len }
                    } else {
                        State::InNalu {
                            len,
                            payload_start_pts: chunk_pts,
                        }
                    };
                }
                State::AtLengthPrefixEnd { .. } => break,
                State::InNalu {
                    len,
                    payload_start_pts,
                } => {
                    if self.buffer.len() < len {
                        break;
                    }
                    let nalu = self.buffer.split_to(len);
                    result.extend(to_output_nalu(&nalu, payload_start_pts));
                    self.state = State::ReadingLengthPrefix;
                }
            }
        }

        result
    }

    pub(super) fn flush(&mut self) -> Vec<(Vec<u8>, Option<u64>)> {
        self.buffer.clear();
        self.state = State::ReadingLengthPrefix;
        Vec::new()
    }
}
