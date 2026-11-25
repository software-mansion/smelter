use gpu_video::parser::h264::{
    AvccPrefixLength, InputBytestreamFormatH264, OutputBytestreamFormatH264, ParsedNalu,
    ParserH264, ParserOptionsH264, nal_types::UnitType,
};

use super::{H264_DUMPS, assert_access_units_eq, parse_h264_ffmpeg, parse_h264_gv, read_file};

/// Stream passed in fixed-size chunks. Start codes and NAL units are split between chunks.
#[test]
fn fixed_size_chunks() {
    for dump_file_path in H264_DUMPS {
        let data = read_file(dump_file_path);
        let expected = parse_h264_ffmpeg(&data);
        for chunk_size in [1, 188, 1400, 4096, data.len()] {
            let actual = parse_h264_gv(
                Default::default(),
                data.chunks(chunk_size).map(|chunk| (chunk, None)),
            )
            .unwrap();
            assert_access_units_eq(
                dump_file_path,
                &format!("chunk size {chunk_size}"),
                &actual,
                &expected,
            );
        }
    }
}

/// Parse access units one by one
#[test]
fn access_unit_chunks_with_flush() {
    for dump_file_path in H264_DUMPS {
        let data = read_file(dump_file_path);
        let expected = parse_h264_ffmpeg(&data);

        let mut parser = ParserH264::default();
        for (idx, expected_au) in expected.iter().enumerate() {
            let pts = Some(idx as u64);
            let mut actual = parser.parse(&expected_au.to_annexb(), pts).unwrap();
            actual.extend(parser.flush().unwrap());

            assert_access_units_eq(
                dump_file_path,
                &format!("flush after access unit {idx}"),
                &actual,
                std::slice::from_ref(expected_au),
            );
            assert!(
                actual[0].0.iter().all(|nalu| nalu.pts == pts),
                "Test {dump_file_path:?}: wrong pts in access unit {idx}"
            );
        }
    }
}

#[test]
fn avcc_output() {
    for dump_file_path in H264_DUMPS {
        let data = read_file(dump_file_path);
        let annexb_aus = parse_h264_gv(Default::default(), [(data.as_slice(), None)]).unwrap();
        let avcc_aus = parse_h264_gv(
            ParserOptionsH264 {
                output_format: OutputBytestreamFormatH264::Avcc,
                ..Default::default()
            },
            [(data.as_slice(), None)],
        )
        .unwrap();

        assert_eq!(avcc_aus.len(), annexb_aus.len(), "Test {dump_file_path:?}");
        for (idx, (avcc_au, annexb_au)) in avcc_aus.iter().zip(&annexb_aus).enumerate() {
            assert_eq!(
                avcc_au.0.len(),
                annexb_au.0.len(),
                "Test {dump_file_path:?}: access unit {idx}"
            );
            for (avcc_nalu, annexb_nalu) in avcc_au.0.iter().zip(&annexb_au.0) {
                assert_avcc_eq(&avcc_nalu.raw_bytes, &annexb_nalu.raw_bytes);
                if let (ParsedNalu::Slice(avcc_slice), ParsedNalu::Slice(annexb_slice)) =
                    (&avcc_nalu.parsed, &annexb_nalu.parsed)
                {
                    assert_avcc_eq(&avcc_slice.rbsp_bytes, &annexb_slice.rbsp_bytes);
                }
            }
        }
    }
}

/// Verifies that each slice uses correct sps
#[test]
fn slices_use_latest_parameter_sets() {
    for dump_file_path in [
        "h264/changing_profile_25fps.h264",
        "h264/changing_resolution_25fps.h264",
    ] {
        let data = read_file(dump_file_path);
        let access_units = parse_h264_gv(
            Default::default(),
            data.chunks(4096).map(|chunk| (chunk, None)),
        )
        .unwrap();

        let mut sps = None;
        let mut pps = None;
        let mut sps_changes = 0;
        for nalu in access_units.iter().flat_map(|au| &au.0) {
            match &nalu.parsed {
                ParsedNalu::Sps(new_sps) => {
                    if sps.is_some_and(|sps| sps != new_sps) {
                        sps_changes += 1;
                    }
                    sps = Some(new_sps);
                }
                ParsedNalu::Pps(new_pps) => pps = Some(format!("{new_pps:?}")),
                ParsedNalu::Slice(slice) => {
                    assert_eq!(Some(&slice.sps), sps, "Test {dump_file_path:?}");
                    assert_eq!(
                        Some(format!("{:?}", slice.pps)),
                        pps,
                        "Test {dump_file_path:?}"
                    );
                }
                ParsedNalu::Other(_) => {}
            }
        }
        assert_eq!(sps_changes, 4, "Test {dump_file_path:?}");
    }
}

#[test]
fn start_code_lengths() {
    for dump_file_path in H264_DUMPS {
        let expected = parse_h264_ffmpeg(&read_file(dump_file_path));
        for (description, start_code) in [
            ("3-byte start codes", [0, 0, 1].as_slice()),
            ("4-byte start codes", &[0, 0, 0, 1]),
            ("trailing zero bytes", &[0, 0, 0, 0, 0, 1]),
        ] {
            let data: Vec<u8> = expected
                .iter()
                .flat_map(|au| &au.nalus)
                .flat_map(|nalu| [start_code, &nalu[4..]].concat())
                .collect();
            let actual = parse_h264_gv(
                Default::default(),
                data.chunks(4096).map(|chunk| (chunk, None)),
            )
            .unwrap();
            assert_access_units_eq(dump_file_path, description, &actual, &expected);
        }
    }
}

#[test]
fn avcc_input() {
    for (prefix_length, max_nalu_len) in [
        (AvccPrefixLength::Bytes2, u16::MAX as usize),
        (AvccPrefixLength::Bytes4, u32::MAX as usize),
    ] {
        for dump_file_path in H264_DUMPS {
            let data = read_file(dump_file_path);
            let expected = parse_h264_ffmpeg(&data);
            let nalus = expected.iter().flat_map(|au| &au.nalus);
            if nalus.clone().any(|nalu| nalu.len() - 4 > max_nalu_len) {
                continue;
            }

            let prefix_byte_length = prefix_length.prefix_byte_length();
            let data: Vec<u8> = nalus
                .flat_map(|nalu| {
                    let payload = &nalu[4..];
                    let len = (payload.len() as u32).to_be_bytes();
                    [&len[4 - prefix_byte_length..], payload].concat()
                })
                .collect();

            let options = ParserOptionsH264 {
                input_format: InputBytestreamFormatH264::Avcc { prefix_length },
                ..Default::default()
            };
            for chunk_size in [1, 1400] {
                let actual =
                    parse_h264_gv(options, data.chunks(chunk_size).map(|chunk| (chunk, None)))
                        .unwrap();
                assert_access_units_eq(
                    dump_file_path,
                    &format!("{prefix_length:?}, chunk size {chunk_size}"),
                    &actual,
                    &expected,
                );
            }
        }
    }
}

// TODO: fix parser?
#[test]
#[ignore = "AU splitter puts EndOfSeq and EndOfStream at the start of the next access unit"]
fn end_of_sequence_and_end_of_stream() {
    let end_of_seq = [0, 0, 0, 1, UnitType::EndOfSeq.id()];
    let end_of_stream = [0, 0, 0, 1, UnitType::EndOfStream.id()];
    for dump_file_path in H264_DUMPS {
        let original = parse_h264_ffmpeg(&read_file(dump_file_path));
        let mut data = Vec::new();
        for (idx, au) in original.iter().enumerate() {
            if idx > 0 && au.is_idr() {
                data.extend(end_of_seq);
            }
            data.extend(au.to_annexb());
        }
        data.extend(end_of_seq);
        data.extend(end_of_stream);

        let expected = parse_h264_ffmpeg(&data);
        let actual = parse_h264_gv(
            Default::default(),
            data.chunks(4096).map(|chunk| (chunk, None)),
        )
        .unwrap();
        assert_access_units_eq(dump_file_path, "whole stream", &actual, &expected);
    }
}

fn assert_avcc_eq(avcc: &[u8], annexb: &[u8]) {
    let len = u32::from_be_bytes(avcc[..4].try_into().unwrap()) as usize;
    assert_eq!(len, avcc.len() - 4);
    assert_eq!(avcc[4..], annexb[4..]);
}
