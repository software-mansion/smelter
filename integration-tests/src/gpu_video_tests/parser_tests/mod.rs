use std::path::Path;

use gpu_video::parser::h264::{AccessUnit, H264Parser, H264ParserError, nal_types::UnitType};

use crate::paths::gpu_video_dumps_dir_path;

mod h264;

const H264_DUMPS: &[&str] = &[
    "h264/big_buck_bunny_10s.h264",
    "h264/changing_profile_25fps.h264",
    "h264/changing_resolution_25fps.h264",
    "h264/frame_cropping_and_stutter.h264",
    "h264/short_reference_deletion.h264",
    "h264/multiple_slices.h264",
    "h264/aud_and_sei.h264",
];

fn read_dump(dump_file_path: &str) -> Vec<u8> {
    std::fs::read(gpu_video_dumps_dir_path().join(dump_file_path)).unwrap()
}

/// Access units produced by the FFmpeg H264 parser, used as the reference.
fn reference_access_units(dump_file_path: &str) -> Vec<Vec<u8>> {
    let path = gpu_video_dumps_dir_path().join(dump_file_path);
    let mut input = ffmpeg_next::format::input(&Path::new(&path)).unwrap();
    input
        .packets()
        .map(|(_, packet)| packet.data().unwrap().to_vec())
        .collect()
}

fn split_nalus(data: &[u8]) -> Vec<Vec<u8>> {
    let mut start_codes = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i..i + 3] == [0, 0, 1] {
            start_codes.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }

    start_codes
        .iter()
        .enumerate()
        .map(|(idx, &start)| {
            let end = start_codes.get(idx + 1).copied().unwrap_or(data.len());
            trim_trailing_zeros(&data[start + 3..end]).to_vec()
        })
        .filter(|nalu| !nalu.is_empty())
        .collect()
}

fn trim_trailing_zeros(data: &[u8]) -> &[u8] {
    let len = data.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    &data[..len]
}

fn access_unit_nalus(au: &AccessUnit) -> Vec<Vec<u8>> {
    au.0.iter()
        .map(|nalu| trim_trailing_zeros(&nalu.raw_bytes[4..]).to_vec())
        .collect()
}

fn describe_nalus(nalus: &[Vec<u8>]) -> Vec<String> {
    nalus
        .iter()
        .map(|nalu| {
            let nal_unit_type = UnitType::for_id(nalu[0] & 0x1f).unwrap();
            format!("{nal_unit_type:?} ({} B)", nalu.len())
        })
        .collect()
}

fn parse_chunks<'a>(
    chunks: impl IntoIterator<Item = (&'a [u8], Option<u64>)>,
) -> Result<Vec<AccessUnit>, H264ParserError> {
    let mut parser = H264Parser::default();
    let mut access_units = Vec::new();
    for (chunk, pts) in chunks {
        access_units.extend(parser.parse(chunk, pts)?);
    }
    access_units.extend(parser.flush()?);
    Ok(access_units)
}

fn assert_access_units_eq(
    dump_file_path: &str,
    context: &str,
    actual: &[AccessUnit],
    expected: &[Vec<u8>],
) {
    for (idx, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let actual = access_unit_nalus(actual);
        let expected = split_nalus(expected);
        assert!(
            actual == expected,
            "Test {dump_file_path:?} ({context}): access unit {idx} differs, (actual) {:?} != (expected) {:?}",
            describe_nalus(&actual),
            describe_nalus(&expected),
        );
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "Test {dump_file_path:?} ({context}): wrong number of access units, (actual) {} != (expected) {}",
        actual.len(),
        expected.len(),
    );
}
