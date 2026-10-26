use gpu_video::parser::h264::{H264Parser, ParsedNalu, nal_types::UnitType};

use super::{
    H264_DUMPS, access_unit_nalus, assert_access_units_eq, parse_chunks, read_dump,
    reference_access_units, split_nalus,
};

/// Whole stream passed in a single call.
#[test]
fn whole_stream() {
    for dump_file_path in H264_DUMPS {
        let data = read_dump(dump_file_path);
        let actual = parse_chunks([(data.as_slice(), None)]).unwrap();
        let expected = reference_access_units(dump_file_path);
        assert_access_units_eq(dump_file_path, "whole stream", &actual, &expected);
    }
}

/// Stream passed in fixed-size chunks, e.g. read from a file or a socket. Start codes and NAL units
/// are split between chunks.
/// Related PRs: #1921
#[test]
fn fixed_size_chunks() {
    for dump_file_path in H264_DUMPS {
        let data = read_dump(dump_file_path);
        let expected = reference_access_units(dump_file_path);
        for chunk_size in [1, 3, 188, 1400, 4096] {
            let actual = parse_chunks(data.chunks(chunk_size).map(|chunk| (chunk, None))).unwrap();
            assert_access_units_eq(
                dump_file_path,
                &format!("chunk size {chunk_size}"),
                &actual,
                &expected,
            );
        }
    }
}

/// One access unit with its pts per call, like MP4 or HLS inputs. Access units are returned with
/// a delay, when the next one starts.
#[test]
fn access_unit_chunks() {
    for dump_file_path in H264_DUMPS {
        let expected = reference_access_units(dump_file_path);
        let actual = parse_chunks(
            expected
                .iter()
                .enumerate()
                .map(|(pts, au)| (au.as_slice(), Some(pts as u64))),
        )
        .unwrap();

        assert_access_units_eq(dump_file_path, "access unit chunks", &actual, &expected);
        for (idx, au) in actual.iter().enumerate() {
            for nalu in au.0.iter() {
                assert_eq!(
                    nalu.pts,
                    Some(idx as u64),
                    "Test {dump_file_path:?}: wrong pts in access unit {idx}"
                );
            }
        }
    }
}

/// One access unit per call followed by a flush, like RTP inputs that signal the end of a frame
/// with the marker bit. Each access unit has to be returned right away.
/// Related PRs: #1854
#[test]
fn access_unit_chunks_with_flush() {
    for dump_file_path in H264_DUMPS {
        let expected = reference_access_units(dump_file_path);
        let mut parser = H264Parser::default();
        for (idx, expected_au) in expected.iter().enumerate() {
            let pts = Some(idx as u64);
            assert!(
                parser.parse(expected_au, pts).unwrap().is_empty(),
                "Test {dump_file_path:?}: access unit {idx} returned before flush"
            );

            let actual = parser.flush().unwrap();
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

/// Each access unit split into RTP-sized chunks, followed by a flush.
#[test]
fn split_access_unit_chunks_with_flush() {
    for dump_file_path in H264_DUMPS {
        let expected = reference_access_units(dump_file_path);
        let mut parser = H264Parser::default();
        for (idx, expected_au) in expected.iter().enumerate() {
            let pts = Some(idx as u64);
            let mut actual = Vec::new();
            for chunk in expected_au.chunks(1200) {
                actual.extend(parser.parse(chunk, pts).unwrap());
            }
            actual.extend(parser.flush().unwrap());

            assert_access_units_eq(
                dump_file_path,
                &format!("flush after access unit {idx}"),
                &actual,
                std::slice::from_ref(expected_au),
            );
        }
    }
}

/// Decoding starts in the middle of a stream, e.g. when joining a live RTP stream. Access units
/// before the first IDR can't be parsed, because SPS and PPS are missing. Everything starting from
/// the first IDR has to be returned.
#[test]
fn join_in_the_middle_of_stream_with_flush() {
    for dump_file_path in H264_DUMPS {
        if *dump_file_path != "h264/aud_and_sei.h264" {
            join_in_the_middle_of_stream(dump_file_path, true);
        }
    }
}

/// Same as [`join_in_the_middle_of_stream_with_flush`], but every access unit starts with AUD and
/// SEI.
#[test]
#[ignore = "NAL units other than slices from access units that failed to parse are kept and \
    returned in the first access unit that parses"]
fn join_in_the_middle_of_stream_with_flush_aud_and_sei() {
    join_in_the_middle_of_stream("h264/aud_and_sei.h264", true);
}

/// Same as [`join_in_the_middle_of_stream_with_flush`], but without flushing, e.g. when joining
/// an MPEG-TS stream.
#[test]
#[ignore = "The slice that fails to parse is returned together with the SPS and PPS of the next \
    access unit, so the error drops them and the parser never recovers"]
fn join_in_the_middle_of_stream_without_flush() {
    for dump_file_path in H264_DUMPS {
        join_in_the_middle_of_stream(dump_file_path, false);
    }
}

fn join_in_the_middle_of_stream(dump_file_path: &str, flush_after_access_unit: bool) {
    let expected = reference_access_units(dump_file_path);
    let start = 10;
    let first_idr = start
        + expected[start..]
            .iter()
            .position(|au| {
                split_nalus(au)
                    .iter()
                    .any(|nalu| nalu[0] & 0x1f == UnitType::SliceLayerWithoutPartitioningIdr.id())
            })
            .unwrap();

    let mut parser = H264Parser::default();
    let mut actual = Vec::new();
    for au in &expected[start..] {
        // Errors are expected until the first IDR.
        actual.extend(parser.parse(au, None).unwrap_or_default());
        if flush_after_access_unit {
            actual.extend(parser.flush().unwrap_or_default());
        }
    }
    actual.extend(parser.flush().unwrap());

    assert_access_units_eq(
        dump_file_path,
        &format!("join at access unit {start}"),
        &actual,
        &expected[first_idr..],
    );
}

/// AVCC output, used by the VideoToolbox decoder.
#[test]
fn avcc_output() {
    for dump_file_path in H264_DUMPS {
        let data = read_dump(dump_file_path);
        let annexb = parse_chunks([(data.as_slice(), None)]).unwrap();

        let mut parser = H264Parser::new_avcc_output();
        let mut avcc = parser.parse(&data, None).unwrap();
        avcc.extend(parser.flush().unwrap());

        assert_eq!(avcc.len(), annexb.len(), "Test {dump_file_path:?}");
        for (idx, (avcc_au, annexb_au)) in avcc.iter().zip(&annexb).enumerate() {
            assert_eq!(
                avcc_au.0.len(),
                annexb_au.0.len(),
                "Test {dump_file_path:?}: access unit {idx}"
            );
            for (avcc_nalu, annexb_nalu) in avcc_au.0.iter().zip(annexb_au.0.iter()) {
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

fn assert_avcc_eq(avcc: &[u8], annexb: &[u8]) {
    let len = u32::from_be_bytes(avcc[..4].try_into().unwrap()) as usize;
    assert_eq!(len, avcc.len() - 4);
    assert_eq!(avcc[4..], annexb[4..]);
}

/// Slices carry the SPS and PPS that were active when they were parsed. Parameter sets are
/// replaced in the middle of these streams with the same ids.
/// Related issues: #1228
#[test]
fn slices_use_latest_parameter_sets() {
    for dump_file_path in [
        "h264/changing_profile_25fps.h264",
        "h264/changing_resolution_25fps.h264",
    ] {
        let data = read_dump(dump_file_path);
        let access_units = parse_chunks(data.chunks(4096).map(|chunk| (chunk, None))).unwrap();

        let mut sps = None;
        let mut pps = None;
        let mut sps_changes = 0;
        for nalu in access_units.iter().flat_map(|au| au.0.iter()) {
            match &nalu.parsed {
                ParsedNalu::Sps(new_sps) => {
                    if sps.as_ref().is_some_and(|sps| sps != new_sps) {
                        sps_changes += 1;
                    }
                    sps = Some(new_sps.clone());
                }
                ParsedNalu::Pps(new_pps) => pps = Some(format!("{new_pps:?}")),
                ParsedNalu::Slice(slice) => {
                    assert_eq!(Some(&slice.sps), sps.as_ref(), "Test {dump_file_path:?}");
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

/// CBR stream with filler data NAL units after slices. Filler data belongs to the access unit
/// of the preceding slice, but the parser moves it to the next one.
#[test]
#[ignore = "NAL units after the last slice are moved to the next access unit"]
fn filler_data() {
    let dump_file_path = "h264/filler_data.h264";
    let data = read_dump(dump_file_path);
    let expected = reference_access_units(dump_file_path);
    let actual = parse_chunks([(data.as_slice(), None)]).unwrap();
    assert_access_units_eq(dump_file_path, "whole stream", &actual, &expected);

    for (idx, au) in actual.iter().enumerate() {
        assert!(
            access_unit_nalus(au)
                .iter()
                .any(|nalu| nalu[0] & 0x1f == UnitType::FillerData.id()),
            "Test {dump_file_path:?}: access unit {idx} has no filler data"
        );
    }
}
