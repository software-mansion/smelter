//! Interactive RTP dump inspection tool.
//!
//! Opens two RTP dumps (expected vs actual) and lets the user step
//! through the paired decoded video frames. Intended to be launched
//! from `audit_tests` after a snapshot mismatch.
//!
//! On launch the inspector spawns a persistent
//! [`crate::tools::frame_inspector`] window that is updated in place
//! every time the playhead advances.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use inquire::{InquireError, Select};
use smelter_render::{Frame, FrameData, YuvPlanes};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{info, warn};

use crate::{
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch},
    tools::{
        frame_inspector::{self, FrameInspector},
        rtp_video_diff_iter::{FramePair, RtpVideoDiffIter, VIDEO_PAYLOAD_TYPE},
        waveform_inspector,
    },
    unmarshal_packets,
};

/// RTP payload type smelter uses for OPUS audio.
const AUDIO_PAYLOAD_TYPE: u8 = 97;
/// OPUS clock rate used everywhere in this crate.
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Launch the interactive inspect tool. Diffs `actual` (the dump
/// just produced by a test run) against `expected` (the committed
/// snapshot). Blocks until the user exits.
pub fn run(expected: &Path, actual: &Path) -> Result<()> {
    info!("rtp_inspector: expected = {}", expected.display());
    info!("rtp_inspector: actual = {}", actual.display());

    let types = scan_payload_types(&[expected, actual])?;
    let mut options = Vec::new();
    if types.contains(&VIDEO_PAYLOAD_TYPE) {
        options.push(MediaKind::Video);
    }
    if types.contains(&AUDIO_PAYLOAD_TYPE) {
        options.push(MediaKind::Audio);
    }
    let kind = match options.len() {
        0 => anyhow::bail!(
            "no video (pt={VIDEO_PAYLOAD_TYPE}) or audio (pt={AUDIO_PAYLOAD_TYPE}) packets in either dump"
        ),
        1 => {
            info!("rtp_inspector: only {} found, skipping prompt", options[0]);
            options[0]
        }
        _ => match Select::new("rtp_inspector — what to inspect?", options).prompt() {
            Ok(k) => k,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        },
    };
    match kind {
        MediaKind::Video => run_video(expected, actual),
        MediaKind::Audio => run_audio(expected, actual),
    }
}

/// Read each dump once and collect the set of RTP payload types
/// present, used to gate the Video / Audio prompt. Missing files are
/// skipped with a warning so the inspector can still launch when one
/// side (typically the committed `expected` snapshot) doesn't exist.
fn scan_payload_types(paths: &[&Path]) -> Result<HashSet<u8>> {
    let mut types = HashSet::new();
    for path in paths {
        if !path.exists() {
            warn!("rtp_inspector: dump {} not found, skipping", path.display());
            continue;
        }
        let bytes = Bytes::from(
            std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?,
        );
        let packets = unmarshal_packets(&bytes)
            .with_context(|| format!("Failed to parse RTP dump {}", path.display()))?;
        for packet in packets {
            types.insert(packet.header.payload_type);
        }
    }
    Ok(types)
}

#[derive(Debug, Clone, Copy, Display, EnumIter)]
enum MediaKind {
    #[strum(to_string = "Video")]
    Video,
    #[strum(to_string = "Audio")]
    Audio,
}

fn run_video(expected: &Path, actual: &Path) -> Result<()> {
    let output_dir = expected
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut iter = RtpVideoDiffIter::from_rtp_dumps(expected, actual)?;
    let mut state = SessionState::default();
    let viewer = FrameInspector::spawn();

    loop {
        let prompt = format!("rtp_inspector [t = {:.3}s]", state.position.as_secs_f64());
        let action = match Select::new(&prompt, Action::iter().collect()).prompt() {
            Ok(a) => a,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        match action {
            Action::NextFrame => advance_one(&mut iter, &mut state, &viewer)?,
            Action::Skip1s => {
                advance_until(&mut iter, &mut state, Duration::from_secs(1), &viewer)?
            }
            Action::Skip5s => {
                advance_until(&mut iter, &mut state, Duration::from_secs(5), &viewer)?
            }
            Action::SaveLastPair => save_last_pair(&state, &output_dir)?,
            Action::Exit => return Ok(()),
        }
    }
}

fn run_audio(expected: &Path, actual: &Path) -> Result<()> {
    let expected_chunks = decode_audio_dump(expected)?;
    let actual_chunks = decode_audio_dump(actual)?;
    waveform_inspector::run(expected_chunks, actual_chunks)
}

/// Read an RTP dump from disk, keep only the OPUS audio packets, and
/// run them all through a fresh decoder. Each decoder output chunk is
/// returned with its original presentation timestamp; chunks are
/// intentionally not flattened so the waveform inspector can show
/// per-chunk boundaries. A missing file yields an empty chunk list
/// rather than an error so the inspector can still surface the other
/// side.
fn decode_audio_dump(path: &Path) -> Result<Vec<AudioSampleBatch>> {
    if !path.exists() {
        warn!(
            "rtp_inspector: audio dump {} not found, treating as empty",
            path.display()
        );
        return Ok(Vec::new());
    }
    let bytes = Bytes::from(
        std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?,
    );
    let packets = unmarshal_packets(&bytes)
        .with_context(|| format!("Failed to parse RTP dump {}", path.display()))?
        .into_iter()
        .filter(|p| p.header.payload_type == AUDIO_PAYLOAD_TYPE);
    let mut decoder = AudioDecoder::new(AUDIO_SAMPLE_RATE, AudioChannels::Stereo)
        .with_context(|| format!("Failed to initialize OPUS decoder for {}", path.display()))?;
    for packet in packets {
        decoder
            .decode(packet)
            .with_context(|| format!("Failed to decode audio packet from {}", path.display()))?;
    }
    Ok(decoder.take_samples())
}

#[derive(Debug, Clone, Copy, Display, EnumIter)]
enum Action {
    #[strum(to_string = "Next frame")]
    NextFrame,
    #[strum(to_string = "Skip 1 second")]
    Skip1s,
    #[strum(to_string = "Skip 5 seconds")]
    Skip5s,
    #[strum(to_string = "Save last pair as PNG")]
    SaveLastPair,
    #[strum(to_string = "Exit")]
    Exit,
}

#[derive(Default)]
struct SessionState {
    /// Latest playhead — the larger of the two side PTS values from
    /// the most recently consumed pair. Drives the prompt and the
    /// skip-by-duration target.
    position: Duration,
    exhausted: bool,
    /// Most recently consumed pair, retained so the user can dump it
    /// to disk via [`Action::SaveLastPair`].
    last_pair: Option<FramePair>,
}

impl SessionState {
    fn ingest(&mut self, pair: FramePair) {
        let left_pts = pair.left.as_ref().map(|f| f.pts).unwrap_or_default();
        let right_pts = pair.right.as_ref().map(|f| f.pts).unwrap_or_default();
        self.position = left_pts.max(right_pts);
        self.last_pair = Some(pair);
    }
}

fn advance_one(
    iter: &mut RtpVideoDiffIter,
    state: &mut SessionState,
    viewer: &FrameInspector,
) -> Result<()> {
    if state.exhausted {
        warn!("end of stream");
        return Ok(());
    }
    match iter.next() {
        Some(pair) => {
            let pair = pair?;
            let mse = pair_mse(&pair);
            log_pair(&pair, mse);
            state.ingest(pair);
            push_to_viewer(state, viewer, mse);
            Ok(())
        }
        None => {
            state.exhausted = true;
            warn!("end of stream");
            Ok(())
        }
    }
}

fn advance_until(
    iter: &mut RtpVideoDiffIter,
    state: &mut SessionState,
    by: Duration,
    viewer: &FrameInspector,
) -> Result<()> {
    if state.exhausted {
        warn!("end of stream");
        return Ok(());
    }
    let target = state.position + by;
    let mut consumed_any = false;
    while state.position < target {
        match iter.next() {
            Some(pair) => {
                let pair = pair?;
                state.ingest(pair);
                consumed_any = true;
            }
            None => {
                state.exhausted = true;
                break;
            }
        }
    }
    if consumed_any {
        let mse = state.last_pair.as_ref().and_then(pair_mse);
        if let Some(pair) = &state.last_pair {
            log_pair(pair, mse);
        }
        push_to_viewer(state, viewer, mse);
    } else {
        warn!("no frames consumed");
    }
    if state.exhausted {
        warn!("end of stream");
    }
    Ok(())
}

/// Convert the most recent pair to RGBA and push it to the viewer
/// thread. Composition (side-by-side / over-under / slider) happens
/// inside the viewer.
fn push_to_viewer(state: &SessionState, viewer: &FrameInspector, mse: Option<f64>) {
    let Some(pair) = state.last_pair.as_ref() else {
        return;
    };
    let (Some(left), Some(right)) = (pair.left.as_ref(), pair.right.as_ref()) else {
        return;
    };
    let left_rgba = match frame_to_rgba(left) {
        Ok(buf) => buf,
        Err(e) => {
            warn!("expected: failed to convert to RGBA: {e:#}");
            return;
        }
    };
    let right_rgba = match frame_to_rgba(right) {
        Ok(buf) => buf,
        Err(e) => {
            warn!("actual: failed to convert to RGBA: {e:#}");
            return;
        }
    };
    viewer.update(frame_inspector::Pair {
        left_label: "expected".to_string(),
        left_caption: format!("pts={:.6}s", left.pts.as_secs_f64()),
        left_rgba,
        left_w: left.resolution.width,
        left_h: left.resolution.height,
        right_label: "actual".to_string(),
        right_caption: format!("pts={:.6}s", right.pts.as_secs_f64()),
        right_rgba,
        right_w: right.resolution.width,
        right_h: right.resolution.height,
        mse,
    });
}

fn save_last_pair(state: &SessionState, output_dir: &Path) -> Result<()> {
    let Some(pair) = state.last_pair.as_ref() else {
        warn!("no frame pair to save — step the inspector first");
        return Ok(());
    };
    let pts_label = format!("{:09}us", state.position.as_micros());
    save_side(pair.left.as_ref(), output_dir, &pts_label, "expected")?;
    save_side(pair.right.as_ref(), output_dir, &pts_label, "actual")?;
    Ok(())
}

fn save_side(frame: Option<&Frame>, output_dir: &Path, pts_label: &str, side: &str) -> Result<()> {
    let Some(frame) = frame else {
        warn!("{side}: no frame at this position, nothing to save");
        return Ok(());
    };
    let rgba = frame_to_rgba(frame).context("Failed to convert frame to RGBA")?;
    let path = output_dir.join(format!("inspect_{pts_label}_{side}.png"));
    image::save_buffer(
        &path,
        &rgba,
        frame.resolution.width as u32,
        frame.resolution.height as u32,
        image::ColorType::Rgba8,
    )
    .with_context(|| format!("Failed to write {}", path.display()))?;
    info!("{side}: wrote {}", path.display());
    Ok(())
}

fn log_pair(pair: &FramePair, mse: Option<f64>) {
    let expected_pts = format_pts(pair.left.as_ref());
    let actual_pts = format_pts(pair.right.as_ref());
    let mse_text = match (pair.left.as_ref(), pair.right.as_ref(), mse) {
        (Some(_), Some(_), Some(v)) => format!("{v:.3}"),
        (Some(_), Some(_), None) => "n/a (incompatible frames)".to_string(),
        _ => "n/a (one side missing)".to_string(),
    };
    info!("frame: expected_pts={expected_pts} actual_pts={actual_pts}");
    info!("mse={mse_text}");
}

fn pair_mse(pair: &FramePair) -> Option<f64> {
    let (e, a) = (pair.left.as_ref()?, pair.right.as_ref()?);
    mean_square_error(e, a)
}

fn format_pts(frame: Option<&Frame>) -> String {
    match frame {
        Some(f) => format!("{:.6}s", f.pts.as_secs_f64()),
        None => "—".to_string(),
    }
}

/// Convert a decoded frame to packed 8-bit RGBA. Only handles the
/// planar YUV formats the H.264 decoder used by the inspector ever
/// produces.
fn frame_to_rgba(frame: &Frame) -> Result<Vec<u8>> {
    let planes = match &frame.data {
        FrameData::PlanarYuv420(p)
        | FrameData::PlanarYuv422(p)
        | FrameData::PlanarYuv444(p)
        | FrameData::PlanarYuvJ420(p) => p,
        other => {
            anyhow::bail!("frame_to_rgba: unsupported frame format {other:?}");
        }
    };
    Ok(yuv420_to_rgba(
        planes,
        frame.resolution.width,
        frame.resolution.height,
    ))
}

/// BT.709 limited-range YUV → RGBA. Mirrors the conversion used by
/// the render-test snapshotting code.
fn yuv420_to_rgba(planes: &YuvPlanes, width: usize, height: usize) -> Vec<u8> {
    // Renderer output is occasionally odd-sized; clamp to even.
    let w = width - (width % 2);
    let h = height - (height % 2);
    let chroma_w = width / 2;

    let mut rgba = Vec::with_capacity(w * h * 4);
    for (i, y_row) in planes.y_plane.chunks(width).enumerate().take(h) {
        for (j, y) in y_row.iter().enumerate().take(w) {
            let mut y = *y as f32;
            let mut u = planes.u_plane[(i / 2) * chroma_w + (j / 2)] as f32;
            let mut v = planes.v_plane[(i / 2) * chroma_w + (j / 2)] as f32;
            y = ((y - 16.0) / 0.858_823_54).clamp(0.0, 255.0);
            u = ((u - 16.0) / 0.878_431_4).clamp(0.0, 255.0);
            v = ((v - 16.0) / 0.878_431_4).clamp(0.0, 255.0);
            let r = (y + 1.5748 * (v - 128.0)).clamp(0.0, 255.0);
            let g = (y - 0.1873 * (u - 128.0) - 0.4681 * (v - 128.0)).clamp(0.0, 255.0);
            let b = (y + 1.8556 * (u - 128.0)).clamp(0.0, 255.0);
            rgba.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
        }
    }
    rgba
}

/// Per-pixel mean square error between two YUV planar frames.
/// Returns `None` when the frames have different resolutions or
/// formats that the inspector doesn't know how to compare.
fn mean_square_error(expected: &Frame, actual: &Frame) -> Option<f64> {
    if expected.resolution != actual.resolution {
        return None;
    }
    let (e, a) = match (&expected.data, &actual.data) {
        (FrameData::PlanarYuv420(e), FrameData::PlanarYuv420(a)) => (e, a),
        (FrameData::PlanarYuv422(e), FrameData::PlanarYuv422(a)) => (e, a),
        (FrameData::PlanarYuv444(e), FrameData::PlanarYuv444(a)) => (e, a),
        (FrameData::PlanarYuvJ420(e), FrameData::PlanarYuvJ420(a)) => (e, a),
        _ => return None,
    };
    let planes = [
        (&e.y_plane, &a.y_plane),
        (&e.u_plane, &a.u_plane),
        (&e.v_plane, &a.v_plane),
    ];
    let mut sum_sq: u64 = 0;
    let mut count: u64 = 0;
    for (lhs, rhs) in planes {
        if lhs.len() != rhs.len() {
            return None;
        }
        for (l, r) in lhs.iter().zip(rhs.iter()) {
            let d = i32::from(*l) - i32::from(*r);
            sum_sq += (d * d) as u64;
        }
        count += lhs.len() as u64;
    }
    if count == 0 {
        return None;
    }
    Some(sum_sq as f64 / count as f64)
}
