//! Play an RTP dump back through GStreamer, auto-detecting which
//! media kinds it contains.
//!
//! Inspects the dump's RTP payload types once, picks one of three
//! `gst-launch-1.0` pipelines (video, audio, or video+audio), and
//! spawns it as a child of `bash -c`. The child is launched with its
//! own process group so the caller can SIGINT the whole subtree (bash
//! → gst-launch and any of its workers) at once when the user asks
//! for playback to stop.

use std::{
    collections::HashSet,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
};

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::unmarshal_packets;

/// RTP payload type smelter uses for H.264 video.
const VIDEO_PAYLOAD_TYPE: u8 = 96;
/// RTP payload type smelter uses for OPUS audio.
const AUDIO_PAYLOAD_TYPE: u8 = 97;

/// Spawn a GStreamer playback pipeline for `path`, detecting which
/// media kinds the dump contains. Returns the spawned `bash` child
/// — the caller is responsible for waiting on it (and may kill its
/// process group via `child.id()` to stop playback).
///
/// `stdin` of the child is set to `Stdio::null` so callers that
/// watch their own stdin (audit_tests's Esc/q key handler) don't
/// race the child for keystrokes.
pub fn spawn(path: &Path) -> Result<Child> {
    let kind = detect(path)?;
    let pipeline = build_pipeline(path, kind);
    Command::new("bash")
        .arg("-c")
        .arg(pipeline)
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .context("Failed to spawn `bash -c gst-launch-1.0 ...`")
}

/// Convenience wrapper around [`spawn`] for callers that just want
/// to block until playback finishes.
pub fn play(path: &Path) -> Result<ExitStatus> {
    spawn(path)?
        .wait()
        .context("Failed to wait on player child")
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    VideoOnly,
    AudioOnly,
    AudioVideo,
}

fn detect(path: &Path) -> Result<StreamKind> {
    let bytes = Bytes::from(
        std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?,
    );
    let packets = unmarshal_packets(&bytes)
        .with_context(|| format!("Failed to parse RTP dump {}", path.display()))?;
    let mut types: HashSet<u8> = HashSet::new();
    for packet in packets {
        types.insert(packet.header.payload_type);
    }
    let has_video = types.contains(&VIDEO_PAYLOAD_TYPE);
    let has_audio = types.contains(&AUDIO_PAYLOAD_TYPE);
    match (has_video, has_audio) {
        (true, true) => Ok(StreamKind::AudioVideo),
        (true, false) => Ok(StreamKind::VideoOnly),
        (false, true) => Ok(StreamKind::AudioOnly),
        (false, false) => anyhow::bail!(
            "no video (pt={VIDEO_PAYLOAD_TYPE}) or audio (pt={AUDIO_PAYLOAD_TYPE}) packets in {}",
            path.display()
        ),
    }
}

fn build_pipeline(path: &Path, kind: StreamKind) -> String {
    let path = path.display();
    match kind {
        StreamKind::VideoOnly => format!(
            "gst-launch-1.0 -v filesrc location={path} ! application/x-rtp-stream ! rtpstreamdepay ! \
             rtph264depay ! video/x-h264,framerate=30/1 ! h264parse ! h264timestamper ! decodebin ! \
             videoconvert ! autovideosink"
        ),
        StreamKind::AudioOnly => format!(
            "gst-launch-1.0 -v filesrc location={path} ! \
             application/x-rtp-stream,payload=97,encoding-name=OPUS ! rtpstreamdepay ! \
             rtpopusdepay ! audio/x-opus ! opusdec ! autoaudiosink"
        ),
        StreamKind::AudioVideo => [
            "gst-launch-1.0 rtpptdemux name=demux ",
            &format!(
                "filesrc location={path} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. "
            ),
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ",
            "! rtph264depay ! decodebin ! videoconvert ! autovideosink ",
            "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ",
            "! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink  ",
        ]
        .concat(),
    }
}
