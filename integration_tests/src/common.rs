use anyhow::{Context, Result};
use bytes::Bytes;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use webrtc_util::Unmarshal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationProtocol {
    Udp,
    Tcp,
}

pub fn input_dump_from_disk<P: AsRef<Path>>(path: P) -> Result<Bytes> {
    let input_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("snapshot_tests")
        .join("snapshots")
        .join("rtp_packet_dumps")
        .join("inputs")
        .join(path);

    let bytes = fs::read(input_path).context("Failed to read input dump")?;
    Ok(Bytes::from(bytes))
}

pub fn output_dump_from_disk<P: AsRef<Path>>(path: P) -> Result<Bytes> {
    let output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("snapshot_tests")
        .join("snapshots")
        .join("rtp_packet_dumps")
        .join("outputs")
        .join(path);

    let bytes = fs::read(output_path).context("Failed to read output dump")?;
    Ok(Bytes::from(bytes))
}

pub fn update_dump_on_disk<P: AsRef<Path>>(path: P, content: &Bytes) -> Result<()> {
    let output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("snapshot_tests")
        .join("snapshots")
        .join("rtp_packet_dumps")
        .join("outputs")
        .join(path);

    fs::write(output_path, content).context("Failed to read output dump")?;
    Ok(())
}

pub fn split_rtp_packet_dump(dump: Bytes, split_at_pts: Duration) -> Result<(Bytes, Bytes)> {
    let mut read_bytes = 0;
    let mut start_pts = None;

    while read_bytes < dump.len() {
        let packet_len = u16::from_be_bytes([dump[read_bytes], dump[read_bytes + 1]]) as usize;
        read_bytes += 2;

        let packet = webrtc::rtp::packet::Packet::unmarshal(
            &mut dump.slice(read_bytes..(read_bytes + packet_len)),
        )?;
        read_bytes += packet_len;

        let packet_pts = match packet.header.payload_type {
            96 => Duration::from_secs_f64(packet.header.timestamp as f64 / 90000.0),
            97 => Duration::from_secs_f64(packet.header.timestamp as f64 / 48000.0),
            payload_type => {
                return Err(anyhow::anyhow!("Unsupported payload type: {payload_type}"))
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("failed_snapshot_tests");

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

pub fn find_packets_for_payload_type(
    packets: &[webrtc::rtp::packet::Packet],
    payload_type: u8,
) -> Vec<webrtc::rtp::packet::Packet> {
    packets
        .iter()
        .filter(|p| p.header.payload_type == payload_type)
        .cloned()
        .collect()
}

pub fn unmarshal_packets(data: &Bytes) -> Result<Vec<webrtc::rtp::packet::Packet>> {
    let mut packets = Vec::new();
    let mut read_bytes = 0;
    while read_bytes < data.len() {
        let packet_size = u16::from_be_bytes([data[read_bytes], data[read_bytes + 1]]) as usize;
        read_bytes += 2;

        if data.len() < read_bytes + packet_size {
            break;
        }

        // TODO(noituri): Goodbye packet
        let packet = webrtc::rtp::packet::Packet::unmarshal(
            &mut &data[read_bytes..(read_bytes + packet_size)],
        )?;
        read_bytes += packet_size;

        packets.push(packet);
    }

    Ok(packets)
}
