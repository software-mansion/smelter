//! Play an MP4 dump back through GStreamer.
//!
//! Counterpart to [`super::rtp_player`] for `.mp4` snapshots. Spawns
//! `gst-launch-1.0 playbin` as a child of `bash -c` with its own
//! process group so the caller can SIGINT the whole subtree (bash →
//! gst-launch and any of its workers) at once when the user asks for
//! playback to stop.

use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
};

use anyhow::{Context, Result};

/// Spawn a GStreamer playback pipeline for `path`. Returns the
/// spawned `bash` child — the caller is responsible for waiting on it
/// (and may kill its process group via `child.id()` to stop playback).
///
/// `stdin` of the child is set to `Stdio::null` so callers that
/// watch their own stdin (audit_tests's Esc/q key handler) don't
/// race the child for keystrokes.
pub fn spawn(path: &Path) -> Result<Child> {
    let path = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", path.display()))?;
    let pipeline = format!("gst-launch-1.0 playbin uri=file://{}", path.display());
    Command::new("bash")
        .arg("-c")
        .arg(pipeline)
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .context("Failed to spawn `bash -c gst-launch-1.0 playbin ...`")
}

/// Convenience wrapper around [`spawn`] for callers that just want
/// to block until playback finishes.
#[allow(dead_code)]
pub fn play(path: &Path) -> Result<ExitStatus> {
    spawn(path)?.wait().context("Failed to wait on player child")
}
