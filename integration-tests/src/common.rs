use anyhow::{Context, Result};
use bytes::Bytes;
use std::{fs, path::Path, time::Duration};
use webrtc::rtp;
use webrtc_util::Unmarshal;

use crate::paths::{pipeline_tests_workdir, submodule_root_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationProtocol {
    Udp,
    Tcp,
}

/// On-disk format of a pipeline-test output dump, derived from the
/// snapshot filename extension via [`dump_format`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    /// Length-prefixed RTP packet dump (H.264 + OPUS).
    Rtp,
    /// MP4 file (H.264 + AAC).
    Mp4,
}

pub fn dump_format<P: AsRef<Path>>(path: P) -> Result<DumpFormat> {
    let path = path.as_ref();
    match path.extension().and_then(|e| e.to_str()) {
        Some("rtp") => Ok(DumpFormat::Rtp),
        Some("mp4") => Ok(DumpFormat::Mp4),
        _ => Err(anyhow::anyhow!(
            "unsupported dump extension in {} (expected .rtp or .mp4)",
            path.display()
        )),
    }
}

pub fn input_dump_from_disk<P: AsRef<Path>>(path: P) -> Result<Bytes> {
    let input_path = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("inputs")
        .join(path);

    let bytes = fs::read(input_path).context("Failed to read input dump")?;
    Ok(Bytes::from(bytes))
}

pub fn output_dump_from_disk<P: AsRef<Path>>(path: P) -> Result<Bytes> {
    let output_path = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("outputs")
        .join(path);

    let bytes = fs::read(output_path).context("Failed to read output dump")?;
    Ok(Bytes::from(bytes))
}

pub fn split_rtp_packet_dump(dump: Bytes, split_at_pts: Duration) -> Result<(Bytes, Bytes)> {
    let mut read_bytes = 0;
    let mut start_pts = None;

    while read_bytes < dump.len() {
        let packet_len = u16::from_be_bytes([dump[read_bytes], dump[read_bytes + 1]]) as usize;
        read_bytes += 2;

        let packet =
            rtp::packet::Packet::unmarshal(&mut dump.slice(read_bytes..(read_bytes + packet_len)))?;
        read_bytes += packet_len;

        let packet_pts = match packet.header.payload_type {
            96 => Duration::from_secs_f64(packet.header.timestamp as f64 / 90000.0),
            97 => Duration::from_secs_f64(packet.header.timestamp as f64 / 48000.0),
            payload_type => {
                return Err(anyhow::anyhow!("Unsupported payload type: {payload_type}"));
            }
        };

        let start_pts = start_pts.get_or_insert(packet_pts);
        if packet_pts.as_micros() - start_pts.as_micros() >= split_at_pts.as_micros() {
            return Ok((dump.slice(0..read_bytes), dump.slice(read_bytes..)));
        }
    }

    Ok((dump, Bytes::new()))
}

pub fn save_failed_test_dumps<P: AsRef<Path>>(
    expected_dump: &Bytes,
    actual_dump: &Bytes,
    snapshot_filename: P,
) {
    let path = pipeline_tests_workdir();

    let _ = fs::create_dir_all(&path);

    let file_name = snapshot_filename
        .as_ref()
        .file_name()
        .unwrap()
        .to_string_lossy();

    fs::write(
        path.join(format!("expected_dump_{file_name}")),
        expected_dump,
    )
    .unwrap();
    fs::write(path.join(format!("actual_dump_{file_name}")), actual_dump).unwrap();
}

/// Write only the `actual` dump to the failed-snapshots dir. Used
/// when the expected snapshot is missing entirely (so there's nothing
/// to pair it with) or in any other path that produced an `actual`
/// without a corresponding `expected` to diff against.
pub fn save_failed_actual_dump<P: AsRef<Path>>(actual_dump: &Bytes, snapshot_filename: P) {
    let path = pipeline_tests_workdir();
    let _ = fs::create_dir_all(&path);
    let file_name = snapshot_filename
        .as_ref()
        .file_name()
        .unwrap()
        .to_string_lossy();
    // This path runs when the expected snapshot doesn't exist, so an
    // expected dump from a previous run is stale — remove it so the
    // audit tooling doesn't pair the fresh actual with old data.
    let stale_expected = path.join(format!("expected_dump_{file_name}"));
    if stale_expected.exists()
        && let Err(e) = fs::remove_file(&stale_expected)
    {
        tracing::warn!(
            "Failed to remove stale expected dump {}: {e}",
            stale_expected.display()
        );
    }
    let dest = path.join(format!("actual_dump_{file_name}"));
    if let Err(e) = fs::write(&dest, actual_dump) {
        tracing::warn!("Failed to write actual dump to {}: {e}", dest.display());
    }
}

pub fn unmarshal_packets(data: &Bytes) -> Result<Vec<rtp::packet::Packet>> {
    let mut packets = Vec::new();
    let mut read_bytes = 0;
    while read_bytes < data.len() {
        let packet_size = u16::from_be_bytes([data[read_bytes], data[read_bytes + 1]]) as usize;
        read_bytes += 2;

        if data.len() < read_bytes + packet_size {
            break;
        }

        // TODO(noituri): Goodbye packet
        let packet =
            rtp::packet::Packet::unmarshal(&mut &data[read_bytes..(read_bytes + packet_size)])?;
        read_bytes += packet_size;

        packets.push(packet);
    }

    Ok(packets)
}
