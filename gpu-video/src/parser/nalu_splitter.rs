use std::sync::LazyLock;

use bytes::{Buf, BufMut, BytesMut};
use memchr::memmem::Finder;

const START_CODE: [u8; 3] = [0, 0, 1];

#[derive(Debug)]
pub(crate) struct NALUSplitter {
    /// Bytes of the current NAL unit after its start code. Outside of a NAL unit, only the bytes
    /// that can still turn out to be the beginning of a start code.
    buffer: BytesMut,
    /// PTS of the chunk where the NAL unit in `buffer` starts.
    pts: Option<u64>,
    previous_search_end: usize,
    /// True while searching for the start code of the next NAL unit. When false, `buffer` holds
    /// the beginning of the current NAL unit, without its start code.
    ///
    /// Some parts of the bitstream don't belong to any NAL unit and need to be dropped:
    /// - Data before the first start code. It can be leading zeros or the remainder of a
    ///   previous NAL unit when the stream is joined in the middle.
    /// - Data after a NAL unit that ended on 0x000000, up to the next start code. That start code
    ///   can begin within those zeros, e.g. in 0x00000001.
    waiting_for_start_code: bool,
}

impl Default for NALUSplitter {
    fn default() -> Self {
        Self {
            buffer: BytesMut::new(),
            pts: None,
            previous_search_end: 0,
            waiting_for_start_code: true,
        }
    }
}

fn find_start_code(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&START_CODE));
    FINDER.find(buf)
}

/// A NAL unit ends before a byte-aligned 0x000001 (start code) or 0x000000, because only zero
/// bytes can follow it (H.264 B.2). h264-reader does the same. FFmpeg ends NAL units only on
/// start codes and trims trailing zeros, so garbage after 0x000000 stays in its NAL unit.
fn find_nalu_end(buf: &[u8]) -> Option<usize> {
    static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(&[0, 0]));
    // Matches don't overlap. A skipped match at `i + 1` needs `buf[i + 2] == 0`, so the match at
    // `i` would be accepted.
    FINDER
        .find_iter(buf)
        .find(|&i| matches!(buf.get(i + 2), Some(0 | 1)))
}

impl NALUSplitter {
    /// Accepts an Annex B byte stream divided into chunks at any byte, chunks don't have to be
    /// aligned to NAL units or start codes.
    pub(crate) fn push(
        &mut self,
        bytestream: &[u8],
        pts: Option<u64>,
    ) -> Vec<(Vec<u8>, Option<u64>)> {
        if self.buffer.is_empty() {
            self.pts = pts;
        }

        self.buffer.put(bytestream);
        let mut result = Vec::new();

        loop {
            if self.waiting_for_start_code {
                // Bytes before first START_CODE or between 0x000000 and next START_CODE
                // should be dropped.
                let Some(bytes_to_drop) = find_start_code(&self.buffer) else {
                    break;
                };

                self.buffer.advance(bytes_to_drop + START_CODE.len());
                self.waiting_for_start_code = false;
                self.pts = pts;
            } else {
                let Some(i) = find_nalu_end(&self.buffer[self.previous_search_end..]) else {
                    break;
                };
                let nalu = self.buffer.split_to(self.previous_search_end + i);
                if let Some(nalu) = nalu_with_start_code(&nalu) {
                    result.push((nalu, self.pts));
                }

                // We can't just drop START_CODE.len() because we don't know if
                // end was START_CODE or 3 zero bytes.
                self.waiting_for_start_code = true;
            }
            self.previous_search_end = 0;
        }

        // A NAL unit end or a start code split between this and the next chunk starts within
        // the last 2 bytes.
        let search_end = self.buffer.len().saturating_sub(START_CODE.len() - 1);
        if self.waiting_for_start_code {
            self.buffer.advance(search_end);
        } else {
            self.previous_search_end = search_end;
        }

        result
    }

    /// Signals the end of the current NAL unit and returns it.
    pub(crate) fn flush(&mut self) -> Vec<(Vec<u8>, Option<u64>)> {
        let mut result = Vec::new();
        if !self.waiting_for_start_code
            && let Some(nalu) = nalu_with_start_code(&self.buffer)
        {
            result.push((nalu, self.pts));
        }

        self.buffer.clear();
        self.previous_search_end = 0;
        self.waiting_for_start_code = true;

        result
    }
}

/// Returns `None` for an empty NAL unit. Trailing zeros at the end of the stream are dropped,
/// they are trailing_zero_8bits, because the last byte of a NAL unit can't be 0.
fn nalu_with_start_code(nalu: &[u8]) -> Option<Vec<u8>> {
    let len = nalu.iter().rposition(|&byte| byte != 0)? + 1;

    let mut result = Vec::with_capacity(len + 4);
    result.extend_from_slice(&[0, 0, 0, 1]);
    result.extend_from_slice(&nalu[..len]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Output = Vec<(Vec<u8>, Option<u64>)>;

    fn split(chunks: &[(&[u8], Option<u64>)]) -> Output {
        let mut splitter = NALUSplitter::default();
        let mut result = Vec::new();
        for (chunk, pts) in chunks {
            result.extend(splitter.push(chunk, *pts));
        }
        result.extend(splitter.flush());
        result
    }

    fn nalus(output: Output) -> Vec<Vec<u8>> {
        output.into_iter().map(|(nalu, _)| nalu).collect()
    }

    #[test]
    fn splits_on_3_and_4_byte_start_codes() {
        let stream = [
            0, 0, 0, 1, 0x67, 0xAA, //
            0, 0, 1, 0x68, 0xBB, //
            0, 0, 0, 1, 0x65, 0xCC,
        ];
        assert_eq!(
            nalus(split(&[(&stream, None)])),
            vec![
                vec![0, 0, 0, 1, 0x67, 0xAA],
                vec![0, 0, 0, 1, 0x68, 0xBB],
                vec![0, 0, 0, 1, 0x65, 0xCC],
            ]
        );
    }

    #[test]
    fn splits_one_byte_nalus() {
        let stream = [0, 0, 0, 1, 0x0A, 0, 0, 1, 0x0B];
        assert_eq!(
            nalus(split(&[(&stream, None)])),
            vec![vec![0, 0, 0, 1, 0x0A], vec![0, 0, 0, 1, 0x0B]]
        );
    }

    #[test]
    fn does_not_split_on_emulation_prevention() {
        let stream = [0, 0, 0, 1, 0x65, 0, 0, 3, 1, 0xAA];
        assert_eq!(nalus(split(&[(&stream, None)])), vec![stream.to_vec()]);
    }

    #[test]
    fn result_does_not_depend_on_chunking() {
        let stream = [
            0xAB, 0, 0, 0, //
            0, 0, 0, 1, 0x67, 0xAA, 0xBB, //
            0, 0, 0, 1, 0x68, 0xCC, 0, 0, 0, 0xCD, //
            0, 0, 1, 0, 0, 0, 1, //
            0, 0, 1, 0x65, 0xDD, 0xEE, //
            0, 0, 1, 0x0A, 0,
        ];
        let expected = vec![
            vec![0, 0, 0, 1, 0x67, 0xAA, 0xBB],
            vec![0, 0, 0, 1, 0x68, 0xCC],
            vec![0, 0, 0, 1, 0x65, 0xDD, 0xEE],
            vec![0, 0, 0, 1, 0x0A],
        ];

        for i in 0..=stream.len() {
            for j in i..=stream.len() {
                let chunks = [
                    (&stream[..i], None),
                    (&stream[i..j], None),
                    (&stream[j..], None),
                ];
                assert_eq!(nalus(split(&chunks)), expected, "split at {i} and {j}");
            }
        }
    }

    #[test]
    fn discards_leading_zero_bytes() {
        let stream = [0, 0, 0, 0, 0, 1, 0x67, 0xAA];
        assert_eq!(
            nalus(split(&[(&stream, None)])),
            vec![vec![0, 0, 0, 1, 0x67, 0xAA]]
        );
    }

    #[test]
    fn discards_data_before_first_start_code() {
        for stream in [
            &[0xAB, 0, 0, 1, 0x67, 0xAA][..],
            &[0xAB, 0xCD, 0xEF, 0, 0, 0, 1, 0x67, 0xAA][..],
            &[0xAB, 0xCD, 0xEF, 0x12, 0x34, 0, 0, 0, 1, 0x67, 0xAA][..],
        ] {
            assert_eq!(
                nalus(split(&[(stream, None)])),
                vec![vec![0, 0, 0, 1, 0x67, 0xAA]],
                "stream {stream:?}"
            );
        }
    }

    #[test]
    fn strips_trailing_zero_bytes() {
        let stream = [
            0, 0, 0, 1, 0x67, 0xAA, 0, 0, //
            0, 0, 0, 1, 0x68, 0xBB, 0, 0,
        ];
        assert_eq!(
            nalus(split(&[(&stream, None)])),
            vec![vec![0, 0, 0, 1, 0x67, 0xAA], vec![0, 0, 0, 1, 0x68, 0xBB]]
        );
    }

    #[test]
    fn ends_nalu_on_three_zero_bytes() {
        let stream = [
            0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 0xCD, 0xEF, //
            0, 0, 0, 1, 0x41, 0xBB,
        ];
        assert_eq!(
            nalus(split(&[(&stream, None)])),
            vec![vec![0, 0, 0, 1, 0x65, 0xAA], vec![0, 0, 0, 1, 0x41, 0xBB]]
        );
    }

    #[test]
    fn flushes_incomplete_data_without_output() {
        for stream in [&[0][..], &[0, 0][..], &[0, 0, 1][..], &[0, 0, 0, 1][..]] {
            assert_eq!(split(&[(stream, None)]), vec![], "stream {stream:?}");
        }
    }

    #[test]
    fn assigns_pts_of_chunk_where_nalu_starts() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x67, 0xAA, 0, 0, 0, 1, 0x68, 0xBB], Some(1)),
            (&[0, 0, 0, 1, 0x65, 0xCC], Some(2)),
        ]);
        assert_eq!(
            output,
            vec![
                (vec![0, 0, 0, 1, 0x67, 0xAA], Some(1)),
                (vec![0, 0, 0, 1, 0x68, 0xBB], Some(1)),
                (vec![0, 0, 0, 1, 0x65, 0xCC], Some(2)),
            ]
        );
    }

    #[test]
    fn assigns_pts_of_chunk_where_nalu_starts_across_many_chunks() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x65, 0x11], Some(1)),
            (&[0x22, 0x33], Some(2)),
            (&[0x44, 0, 0, 0, 1, 0x41, 0x55], Some(3)),
        ]);
        assert_eq!(
            output,
            vec![
                (vec![0, 0, 0, 1, 0x65, 0x11, 0x22, 0x33, 0x44], Some(1)),
                (vec![0, 0, 0, 1, 0x41, 0x55], Some(3)),
            ]
        );
    }

    #[test]
    fn assigns_pts_of_chunk_where_nalu_starts_on_flush() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x65, 0x11], Some(1)),
            (&[0x22, 0x33], Some(2)),
        ]);
        assert_eq!(
            output,
            vec![(vec![0, 0, 0, 1, 0x65, 0x11, 0x22, 0x33], Some(1))]
        );
    }

    #[test]
    fn assigns_pts_of_chunk_with_payload_when_start_code_ends_chunk() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x67, 0xAA, 0, 0, 0, 1], Some(1)),
            (&[0x65, 0xBB], Some(2)),
        ]);
        assert_eq!(
            output,
            vec![
                (vec![0, 0, 0, 1, 0x67, 0xAA], Some(1)),
                (vec![0, 0, 0, 1, 0x65, 0xBB], Some(2)),
            ]
        );
    }

    #[test]
    fn assigns_pts_when_start_code_is_split_after_three_zero_bytes() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x67, 0xAA, 0, 0, 0], Some(1)),
            (&[1, 0x65, 0xBB], Some(2)),
        ]);
        assert_eq!(
            output,
            vec![
                (vec![0, 0, 0, 1, 0x67, 0xAA], Some(1)),
                (vec![0, 0, 0, 1, 0x65, 0xBB], Some(2)),
            ]
        );
    }

    #[test]
    fn assigns_pts_of_chunk_with_start_code_after_garbage() {
        let output = split(&[
            (&[0, 0, 0, 1, 0x67, 0xAA, 0, 0, 0, 0xCD], Some(1)),
            (&[0xEF, 0, 0, 1, 0x65, 0xBB], Some(2)),
        ]);
        assert_eq!(
            output,
            vec![
                (vec![0, 0, 0, 1, 0x67, 0xAA], Some(1)),
                (vec![0, 0, 0, 1, 0x65, 0xBB], Some(2)),
            ]
        );
    }

    #[test]
    fn flush_resets_state() {
        let mut splitter = NALUSplitter::default();
        splitter.push(&[0, 0, 0, 1, 0x65, 0xAA], Some(1));
        splitter.flush();

        splitter.push(&[0, 0, 0, 1, 0x41, 0xBB], Some(2));
        assert_eq!(
            splitter.flush(),
            vec![(vec![0, 0, 0, 1, 0x41, 0xBB], Some(2))]
        );
    }
}
