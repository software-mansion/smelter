use std::sync::LazyLock;

use bytes::{Buf, BufMut, BytesMut};
use memchr::memmem::Finder;

use super::to_output_nalu;

const START_CODE: [u8; 3] = [0, 0, 1];
const BOUNDARY_PATTERN_LEN: usize = START_CODE.len();

#[derive(Debug, Default)]
enum State {
    #[default]
    DiscardingUntilStartCode,
    AtStartCodeEnd,
    InNalu {
        payload_start_pts: Option<u64>,
        resume_search_at: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct AnnexBSplitter {
    buffer: BytesMut,
    state: State,
}

impl AnnexBSplitter {
    pub(super) fn push(
        &mut self,
        chunk: &[u8],
        chunk_pts: Option<u64>,
    ) -> Vec<(Vec<u8>, Option<u64>)> {
        if let State::AtStartCodeEnd = self.state
            && !chunk.is_empty()
        {
            self.state = State::InNalu {
                payload_start_pts: chunk_pts,
                resume_search_at: 0,
            };
        }

        self.buffer.put(chunk);
        let mut result = Vec::new();

        loop {
            match self.state {
                State::DiscardingUntilStartCode => {
                    if !self.skip_past_start_code(chunk_pts) {
                        break;
                    }
                }
                State::AtStartCodeEnd => break,
                State::InNalu {
                    payload_start_pts,
                    resume_search_at,
                } => {
                    let Some(nalu) = self.split_off_nalu(resume_search_at) else {
                        break;
                    };
                    result.extend(to_output_nalu(&nalu, payload_start_pts));
                }
            }
        }

        let fully_searched_len = self.buffer.len().saturating_sub(BOUNDARY_PATTERN_LEN - 1);
        match &mut self.state {
            State::DiscardingUntilStartCode => self.buffer.advance(fully_searched_len),
            State::AtStartCodeEnd => {}
            State::InNalu {
                resume_search_at, ..
            } => *resume_search_at = fully_searched_len,
        }

        result
    }

    pub(super) fn flush(&mut self) -> Vec<(Vec<u8>, Option<u64>)> {
        let mut result = Vec::new();
        if let State::InNalu {
            payload_start_pts, ..
        } = self.state
        {
            result.extend(to_output_nalu(&self.buffer, payload_start_pts));
        }

        self.buffer.clear();
        self.state = State::DiscardingUntilStartCode;

        result
    }

    fn skip_past_start_code(&mut self, chunk_pts: Option<u64>) -> bool {
        let Some(start_code_position) = find_start_code(&self.buffer) else {
            return false;
        };

        self.buffer.advance(start_code_position + START_CODE.len());
        self.state = if self.buffer.is_empty() {
            State::AtStartCodeEnd
        } else {
            State::InNalu {
                payload_start_pts: chunk_pts,
                resume_search_at: 0,
            }
        };
        true
    }

    fn split_off_nalu(&mut self, resume_search_at: usize) -> Option<BytesMut> {
        let nalu_len =
            resume_search_at + find_start_code_or_zero_padding(&self.buffer[resume_search_at..])?;
        self.state = State::DiscardingUntilStartCode;
        Some(self.buffer.split_to(nalu_len))
    }
}

fn find_start_code(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&START_CODE));
    FINDER.find(buf)
}

fn find_start_code_or_zero_padding(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&[0, 0]));
    // Matches don't overlap. A skipped match at `i + 1` needs `buf[i + 2] == 0`, so the match at
    // `i` would be accepted.
    FINDER
        .find_iter(buf)
        .find(|&i| matches!(buf.get(i + 2), Some(0 | 1)))
}
