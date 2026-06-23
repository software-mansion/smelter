//! Interactive inspector for pipeline-test output dumps.
//!
//! Opens two dumps (expected vs actual) — either `.rtp` packet dumps
//! or `.mp4` files — and lets the user step through the paired
//! decoded video frames. Launched from the audit menu after a
//! snapshot mismatch.
//!
//! On launch the inspector spawns a persistent
//! [`frame_inspector`] window that is updated in place every time the
//! playhead advances.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use inquire::{InquireError, Select};
use integration_tests::{
    AudioSampleBatch, DumpFormat, dump_format,
    tools::{
        frame_inspector::{self, FrameInspector},
        mp4_source,
        pixel_format::{frame_to_rgba, mean_square_error},
        rtp_source::{self, MediaKind, available_media_kinds},
        video_diff_iter::{FramePair, VideoDiffIter},
        waveform_inspector,
    },
};
use smelter_render::{Frame, Resolution};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{info, warn};

/// Audio sample rate the inspector decodes to — the OPUS clock rate.
/// AAC tracks in MP4 dumps must be encoded at this rate too.
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Launch the interactive inspect tool. Diffs `actual` (the dump
/// just produced by a test run) against `expected` (the committed
/// snapshot). Both dumps must share the format implied by their file
/// extension. Blocks until the user exits.
pub(crate) fn run(expected: &Path, actual: &Path) -> Result<()> {
    info!("inspector: expected = {}", expected.display());
    info!("inspector: actual = {}", actual.display());

    // Both sides are the same snapshot under different prefixes, so
    // either path yields the same format.
    let format = dump_format(actual)?;
    let options = available_media_kinds(format, &[expected, actual])?;
    let kind = match options.len() {
        0 => anyhow::bail!("no video or audio found in either dump"),
        1 => {
            info!("inspector: only {} found, skipping prompt", options[0]);
            options[0]
        }
        _ => match Select::new("inspector — what to inspect?", options).prompt() {
            Ok(k) => k,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        },
    };
    match kind {
        MediaKind::Video => run_video(expected, actual, format),
        MediaKind::Audio => run_audio(expected, actual, format),
    }
}

fn run_video(expected: &Path, actual: &Path, format: DumpFormat) -> Result<()> {
    let output_dir =
        expected.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let mut iter = new_video_diff_iter(expected, actual, format)?;
    let mut state = SessionState::default();
    let viewer = FrameInspector::spawn();

    // Pull the first pair eagerly so the viewer window opens with
    // something on screen, rather than waiting for the user to pick
    // an action just to see anything at all.
    advance_one(&mut iter, &mut state, &viewer)?;

    loop {
        let prompt = format!("inspector [t = {:.3}s]", state.position.as_secs_f64());
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
            Action::NextHighMse => {
                advance_until_high_mse(&mut iter, &mut state, &viewer)?
            }
            Action::Restart => {
                iter = new_video_diff_iter(expected, actual, format)?;
                state = SessionState::default();
                advance_one(&mut iter, &mut state, &viewer)?;
            }
            Action::SaveLastPair => save_last_pair(&state, &output_dir)?,
            Action::Exit => return Ok(()),
        }
    }
}

/// The diff iterator can only move forward; restarting playback means
/// building a fresh one over the same dumps.
fn new_video_diff_iter(
    expected: &Path,
    actual: &Path,
    format: DumpFormat,
) -> Result<VideoDiffIter> {
    match format {
        DumpFormat::Rtp => VideoDiffIter::from_rtp_dumps(expected, actual),
        DumpFormat::Mp4 => VideoDiffIter::from_mp4_dumps(expected, actual),
    }
}

fn run_audio(expected: &Path, actual: &Path, format: DumpFormat) -> Result<()> {
    let expected_chunks = decode_audio_dump(expected, format)?;
    let actual_chunks = decode_audio_dump(actual, format)?;
    waveform_inspector::run(expected_chunks, actual_chunks)
}

/// Read a dump from disk and decode its audio track (OPUS for `.rtp`
/// dumps, AAC for `.mp4` dumps). Each decoder output chunk is
/// returned with its original presentation timestamp; chunks are
/// intentionally not flattened so the waveform inspector can show
/// per-chunk boundaries. A missing file yields an empty chunk list
/// rather than an error so the inspector can still surface the other
/// side.
fn decode_audio_dump(path: &Path, format: DumpFormat) -> Result<Vec<AudioSampleBatch>> {
    if !path.exists() {
        warn!("inspector: audio dump {} not found, treating as empty", path.display());
        return Ok(Vec::new());
    }
    let bytes = Bytes::from(
        std::fs::read(path)
            .with_context(|| format!("Failed to read {}", path.display()))?,
    );
    match format {
        DumpFormat::Rtp => rtp_source::decode_opus_audio(&bytes, AUDIO_SAMPLE_RATE)
            .with_context(|| format!("Failed to decode audio from {}", path.display())),
        DumpFormat::Mp4 => mp4_source::decode_aac_audio(&bytes, AUDIO_SAMPLE_RATE)
            .with_context(|| format!("Failed to decode audio from {}", path.display())),
    }
}

/// MSE threshold used by [`Action::NextHighMse`]. Anything below this
/// is small enough to be encoder noise; anything above is a visible
/// difference worth showing.
const HIGH_MSE_THRESHOLD: f64 = 1.0;

#[derive(Debug, Clone, Copy, Display, EnumIter)]
enum Action {
    #[strum(to_string = "Next frame")]
    NextFrame,
    #[strum(to_string = "Skip 1 second")]
    Skip1s,
    #[strum(to_string = "Skip 5 seconds")]
    Skip5s,
    #[strum(to_string = "Next frame with mse > 1")]
    NextHighMse,
    #[strum(to_string = "Start from the beginning")]
    Restart,
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
    iter: &mut VideoDiffIter,
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
    iter: &mut VideoDiffIter,
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

/// Pull pairs until one has both sides present and an MSE above
/// [`HIGH_MSE_THRESHOLD`], then surface that pair. Skipped pairs are
/// silently consumed — only the landing pair is logged and shown.
fn advance_until_high_mse(
    iter: &mut VideoDiffIter,
    state: &mut SessionState,
    viewer: &FrameInspector,
) -> Result<()> {
    if state.exhausted {
        warn!("end of stream");
        return Ok(());
    }
    let mut skipped: usize = 0;
    loop {
        match iter.next() {
            Some(pair) => {
                let pair = pair?;
                let mse = pair_mse(&pair);
                state.ingest(pair);
                if mse.is_some_and(|v| v > HIGH_MSE_THRESHOLD) {
                    if skipped > 0 {
                        info!("skipped {skipped} low-mse pair(s)");
                    }
                    if let Some(pair) = &state.last_pair {
                        log_pair(pair, mse);
                    }
                    push_to_viewer(state, viewer, mse);
                    return Ok(());
                }
                skipped += 1;
            }
            None => {
                state.exhausted = true;
                if skipped > 0 {
                    warn!(
                        "end of stream — no pair with mse > {HIGH_MSE_THRESHOLD} found in \
                         the remaining {skipped} pair(s)"
                    );
                } else {
                    warn!("end of stream");
                }
                return Ok(());
            }
        }
    }
}

/// Convert the most recent pair to RGBA and push it to the viewer
/// thread. Composition (side-by-side / over-under / slider) happens
/// inside the viewer. A side without a frame (stream exhausted, or
/// the dump missing entirely — e.g. no committed snapshot yet) is
/// shown as a placeholder so the other side is still inspectable.
fn push_to_viewer(state: &SessionState, viewer: &FrameInspector, mse: Option<f64>) {
    let Some(pair) = state.last_pair.as_ref() else {
        return;
    };
    // The iterator never yields a pair with both sides missing, so
    // there is always a resolution to size the placeholder after.
    let Some(placeholder_resolution) =
        pair.left.as_ref().or(pair.right.as_ref()).map(|f| f.resolution)
    else {
        return;
    };
    let Some(left) = side_view(pair.left.as_ref(), "expected", placeholder_resolution)
    else {
        return;
    };
    let Some(right) = side_view(pair.right.as_ref(), "actual", placeholder_resolution)
    else {
        return;
    };
    viewer.update(frame_inspector::Pair {
        left_label: "expected".to_string(),
        left_lines: left.lines,
        left_rgba: left.rgba,
        left_w: left.width,
        left_h: left.height,
        right_label: "actual".to_string(),
        right_lines: right.lines,
        right_rgba: right.rgba,
        right_w: right.width,
        right_h: right.height,
        mse,
    });
}

struct SideView {
    rgba: Vec<u8>,
    width: usize,
    height: usize,
    lines: Vec<String>,
}

/// One side of a viewer update. `None` only when an existing frame
/// fails to convert to RGBA; a missing frame becomes a dark-gray
/// placeholder captioned "(no frame)".
fn side_view(
    frame: Option<&Frame>,
    side: &str,
    placeholder: Resolution,
) -> Option<SideView> {
    match frame {
        Some(frame) => match frame_to_rgba(frame) {
            Ok(rgba) => Some(SideView {
                rgba,
                width: frame.resolution.width,
                height: frame.resolution.height,
                lines: vec![format!("pts={:.6}s", frame.pts.as_secs_f64())],
            }),
            Err(e) => {
                warn!("{side}: failed to convert to RGBA: {e:#}");
                None
            }
        },
        None => Some(SideView {
            rgba: [40, 40, 40, 255].repeat(placeholder.width * placeholder.height),
            width: placeholder.width,
            height: placeholder.height,
            lines: vec!["(no frame)".to_string()],
        }),
    }
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

fn save_side(
    frame: Option<&Frame>,
    output_dir: &Path,
    pts_label: &str,
    side: &str,
) -> Result<()> {
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
