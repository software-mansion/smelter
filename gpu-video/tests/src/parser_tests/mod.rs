use gpu_video::parser::h264::{
    ParserErrorH264, ParserH264, ParserOptionsH264, nal_types::UnitType,
};

use crate::{gpu_video_dumps_dir_path, harness::decoders::FfmpegParser};

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

fn read_file(dump_file_path: &str) -> Vec<u8> {
    std::fs::read(gpu_video_dumps_dir_path().join(dump_file_path)).unwrap()
}

fn parse_h264_ffmpeg(data: &[u8]) -> Vec<AccessUnitFfmpeg> {
    let mut parser = FfmpegParser::new(ffmpeg_next::codec::Id::H264);
    let mut access_units = parser.parse(data);
    access_units.extend(parser.flush());
    access_units.iter().map(AccessUnitFfmpeg::new).collect()
}

fn parse_h264_gv<'a>(
    options: ParserOptionsH264,
    chunks: impl IntoIterator<Item = (&'a [u8], Option<u64>)>,
) -> Result<Vec<AccessUnitGv>, ParserErrorH264> {
    let mut parser = ParserH264::new(options);
    let mut access_units = Vec::new();
    for (chunk, pts) in chunks {
        access_units.extend(parser.parse(chunk, pts)?);
    }
    access_units.extend(parser.flush()?);
    Ok(access_units)
}

fn split_nalus(mut data: &[u8]) -> Vec<Vec<u8>> {
    let mut nalus = Vec::new();
    while let Some(start) = find_start_code(data) {
        data = &data[start + 3..];
        let end = find_start_code(data).unwrap_or(data.len());
        let payload = strip_trailing_zero_8bits(&data[..end]);
        if !payload.is_empty() {
            nalus.push([&[0, 0, 0, 1], payload].concat());
        }
        data = &data[end..];
    }
    nalus
}

fn find_start_code(data: &[u8]) -> Option<usize> {
    data.windows(3).position(|window| window == [0, 0, 1])
}

fn strip_trailing_zero_8bits(bytes: &[u8]) -> &[u8] {
    let len = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |last_nonzero| last_nonzero + 1);
    &bytes[..len]
}

fn describe_nalus(nalus: &[&[u8]]) -> Vec<String> {
    nalus
        .iter()
        .map(|nalu| format!("{:?} ({} B)", nal_unit_type(nalu), nalu.len()))
        .collect()
}

fn nal_unit_type(nalu: &[u8]) -> UnitType {
    UnitType::for_id(nalu[4] & 0x1f).unwrap()
}

fn assert_access_units_eq(
    dump_file_path: &str,
    context: &str,
    actual: &[AccessUnitGv],
    expected: &[AccessUnitFfmpeg],
) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "Test {dump_file_path:?} ({context}): wrong number of access units, (actual) {} != (expected) {}",
        actual.len(),
        expected.len(),
    );

    for (idx, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let actual: Vec<&[u8]> = actual.0.iter().map(|nalu| &*nalu.raw_bytes).collect();
        let expected: Vec<&[u8]> = expected.nalus.iter().map(Vec::as_slice).collect();
        assert!(
            actual == expected,
            "Test {dump_file_path:?} ({context}): access unit {idx} differs, (actual) {:?} != (expected) {:?}",
            describe_nalus(&actual),
            describe_nalus(&expected),
        );
    }
}

struct AccessUnitFfmpeg {
    nalus: Vec<Vec<u8>>,
}

impl AccessUnitFfmpeg {
    fn new(packet: &ffmpeg_next::Packet) -> Self {
        Self {
            nalus: split_nalus(packet.data().unwrap()),
        }
    }

    fn to_annexb(&self) -> Vec<u8> {
        self.nalus.concat()
    }

    fn is_idr(&self) -> bool {
        self.nalus
            .iter()
            .any(|nalu| nal_unit_type(nalu) == UnitType::SliceLayerWithoutPartitioningIdr)
    }
}

type AccessUnitGv = gpu_video::parser::h264::AccessUnit;
