mod annexb;
mod avcc;

use super::h264::InputNaluFormatH264;
use annexb::AnnexBSplitter;
use avcc::AvccSplitter;

const OUTPUT_START_CODE: [u8; 4] = [0, 0, 0, 1];

#[derive(Debug)]
pub(crate) enum NALUSplitter {
    AnnexB(AnnexBSplitter),
    Avcc(AvccSplitter),
}

impl Default for NALUSplitter {
    fn default() -> Self {
        Self::new(InputNaluFormatH264::default())
    }
}

impl NALUSplitter {
    pub(crate) fn new(bitstream_format: InputNaluFormatH264) -> Self {
        match bitstream_format {
            InputNaluFormatH264::AnnexB => Self::AnnexB(AnnexBSplitter::default()),
            InputNaluFormatH264::Avcc { prefix_length } => {
                Self::Avcc(AvccSplitter::new(prefix_length))
            }
        }
    }

    pub(crate) fn push(
        &mut self,
        chunk: &[u8],
        chunk_pts: Option<u64>,
    ) -> Vec<(Vec<u8>, Option<u64>)> {
        match self {
            Self::AnnexB(splitter) => splitter.push(chunk, chunk_pts),
            Self::Avcc(splitter) => splitter.push(chunk, chunk_pts),
        }
    }

    pub(crate) fn flush(&mut self) -> Vec<(Vec<u8>, Option<u64>)> {
        match self {
            Self::AnnexB(splitter) => splitter.flush(),
            Self::Avcc(splitter) => splitter.flush(),
        }
    }
}

fn to_output_nalu(nalu: &[u8], pts: Option<u64>) -> Option<(Vec<u8>, Option<u64>)> {
    let payload = strip_trailing_zero_8bits(nalu);
    if payload.is_empty() {
        return None;
    }
    Some(([&OUTPUT_START_CODE[..], payload].concat(), pts))
}

fn strip_trailing_zero_8bits(bytes: &[u8]) -> &[u8] {
    let len = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |last_nonzero| last_nonzero + 1);
    &bytes[..len]
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
