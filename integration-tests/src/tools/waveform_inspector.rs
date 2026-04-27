//! Interactive waveform inspection tool — audio counterpart to
//! [`crate::tools::rtp_inspector`]'s video flow.
//!
//! Receives the fully-decoded expected and actual streams as a list of
//! decoder chunks (each carrying its original timestamp). Chunks are
//! placed on a contiguous time-indexed buffer using their `pts` so any
//! gaps in the input become silence.
//!
//! ## Display
//!
//! Four stacked envelope lanes sharing one time axis covering the full
//! duration: `expected L`, `expected R`, `actual L`, `actual R`.
//!
//! Below them, a mouse-X-driven zoom region shows two strips (one per
//! channel), each overlaying the actual sample values of expected and
//! actual around the cursor in their respective colours, so phase /
//! sample-level differences are visible. The cursor is a vertical
//! line spanning the six main lanes.

use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::Result;
use font8x8::UnicodeFonts;
use inquire::{InquireError, Select};
use minifb::{Key, MouseMode, ScaleMode, Window, WindowOptions};
use spectrum_analyzer::{
    FrequencyLimit, FrequencySpectrum, samples_fft_to_spectrum, scaling::divide_by_N,
    windows::hann_window,
};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{error, info};

use crate::audio_decoder::AudioSampleBatch;

const SAMPLE_RATE: u32 = 48_000;

/// Width of the window when first opened. After that the buffer
/// tracks the window size on every frame so resizing the window grows
/// (or shrinks) the rendered area horizontally.
const INIT_W: usize = 1280;
/// Minimum buffer width we'll render at — guards against degenerate
/// envelope computations when the user squashes the window.
const MIN_W: usize = 64;
const HEADER_H: usize = 28;
const LANE_H: usize = 80;
const GAP_LANE_H: usize = 36;
const PRIMARY_FREQ_LANE_H: usize = 70;
const LANE_GAP: usize = 4;
const ZOOM_H: usize = 110;
const SPECTRUM_H: usize = 120;
/// FFT window for both the precomputed centroid time series and the
/// cursor-driven spectrum strip. Power of two as required by the
/// `spectrum-analyzer` crate.
const STFT_WINDOW: usize = 2048;
/// Hop between consecutive STFT frames for the centroid time series.
/// 512 samples ≈ 10.7 ms at 48 kHz — fine enough for a clear line.
const STFT_HOP: usize = 512;
/// Cursor spectrum strip y-axis range. dB values below this are
/// clamped to the bottom of the strip.
const SPECTRUM_DB_FLOOR: f32 = -80.0;
/// Vertical space reserved at the top of every lane / zoom strip for
/// the label so the envelope or plot below it never collides with the
/// text. One row of the 8×8 font at `FONT_SCALE` + a few pixels of pad.
const LABEL_BAND_H: usize = 8 * FONT_SCALE + 6;
const NUM_LANES: usize = 4;
const NUM_GAP_LANES: usize = 2;
const NUM_ARTIFACT_LANES: usize = 2;
/// Gaps shorter than this are ignored — they're well within the noise
/// of decoder timing jitter and would just clutter the display.
const GAP_THRESHOLD: Duration = Duration::from_micros(50);
/// Half-window (in samples) over which the artifact detector averages
/// `|d1|` to form the local baseline. ~1.3 ms at 48 kHz — small
/// enough that the baseline tracks fast dynamic changes (so loud
/// transients don't generate false positives because the surrounding
/// audio still looks quiet).
const ARTIFACT_WINDOW_RADIUS: usize = 64;
/// Multiplier on the local mean of `|d1|` above which a sample is
/// flagged as a step (C0 discontinuity). Real decoder glitches
/// usually produce ratios well into double digits; values just above
/// this threshold tend to be sharp-but-legitimate transients.
const ARTIFACT_D1_MULT: f32 = 7.0;
/// Absolute floor (as fraction of global peak) below which a candidate
/// is ignored even if the relative threshold fires. Keeps quantisation
/// noise in silent regions from generating endless false positives.
const ARTIFACT_D1_FLOOR_FRAC: f32 = 0.05;
/// Flagged samples within this many samples of each other are merged
/// into a single interval (so a 1–3 sample click reads as one bar).
const ARTIFACT_MERGE_GAP: usize = 64;
/// Logical height of the laid-out content. The buffer can be taller
/// (extra space at the bottom is just background) or shorter (content
/// past the bottom edge is clipped — the layout itself is fixed).
const CANVAS_H: usize = HEADER_H
    + NUM_LANES * LANE_H
    + (NUM_LANES - 1) * LANE_GAP
    + LANE_GAP
    + NUM_GAP_LANES * GAP_LANE_H
    + (NUM_GAP_LANES - 1) * LANE_GAP
    + LANE_GAP
    + NUM_ARTIFACT_LANES * GAP_LANE_H
    + (NUM_ARTIFACT_LANES - 1) * LANE_GAP
    + LANE_GAP
    + PRIMARY_FREQ_LANE_H
    + LANE_GAP
    + 2 * ZOOM_H
    + LANE_GAP
    + LANE_GAP
    + SPECTRUM_H
    + LANE_GAP;

const BG: u32 = 0;
const AXIS: u32 = 0x0033_3333;
const CURSOR: u32 = 0x00FF_FFFF;
const COLOR_EXPECTED: u32 = 0x0040_FF60;
const COLOR_ACTUAL: u32 = 0x0040_C0FF;
const COLOR_GAP: u32 = 0x00FF_5050;
const COLOR_ARTIFACT: u32 = 0x00FF_C040;
const TEXT: u32 = 0x00FF_FFFF;
const FONT_SCALE: usize = 2;
const GLYPH_W: usize = 8 * FONT_SCALE + FONT_SCALE;

/// Half-window of the zoom strips in seconds.
const ZOOM_HALF_WINDOW: f32 = 0.05;

/// Channel index used for the left side of the stereo signal.
const CH_L: usize = 0;
/// Channel index used for the right side of the stereo signal.
const CH_R: usize = 1;

/// Launch the interactive waveform inspector. Blocks until the user
/// exits. `expected` and `actual` are the decoded chunk streams from
/// the two RTP dumps, with timestamps preserved per chunk.
pub fn run(expected: Vec<AudioSampleBatch>, actual: Vec<AudioSampleBatch>) -> Result<()> {
    info!(
        "waveform_inspector: expected={} chunks, actual={} chunks",
        expected.len(),
        actual.len(),
    );

    let viewer = WaveformViewer::spawn(expected, actual);

    loop {
        let action = match Select::new("waveform_inspector", Action::iter().collect()).prompt() {
            Ok(a) => a,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                drop(viewer);
                return Ok(());
            }
            Err(e) => {
                drop(viewer);
                return Err(e.into());
            }
        };
        match action {
            Action::ShowFull => viewer.send(ViewCommand::ShowFull),
            Action::ShowOneSecond => viewer.send(ViewCommand::ShowOneSecond),
            Action::Exit => {
                drop(viewer);
                return Ok(());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Display, EnumIter)]
enum Action {
    #[strum(to_string = "Show full")]
    ShowFull,
    #[strum(to_string = "Show 1-sec window")]
    ShowOneSecond,
    #[strum(to_string = "Exit")]
    Exit,
}

/// Commands the inquire menu sends to the viewer thread to drive the
/// envelope view range. Mouse-wheel zoom in the window still works
/// alongside these. `ShowOneSecond` snaps to a 1-second window
/// starting at 0 when the view isn't already 1 sec wide; otherwise it
/// advances to the next 1-second slot, so repeated invocations walk
/// forward second-by-second.
#[derive(Debug, Clone, Copy)]
enum ViewCommand {
    ShowFull,
    ShowOneSecond,
}

/// Handle to the spawned viewer thread. Dropping it closes the
/// shutdown channel, which tells the thread to exit on its next poll.
struct WaveformViewer {
    join: Option<JoinHandle<()>>,
    stop: Option<Sender<()>>,
    cmd: Option<Sender<ViewCommand>>,
}

impl WaveformViewer {
    fn spawn(expected: Vec<AudioSampleBatch>, actual: Vec<AudioSampleBatch>) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<ViewCommand>();
        let state = ViewerState::build(expected, actual);
        let join = thread::Builder::new()
            .name("waveform_inspector".into())
            .spawn(move || run_window(state, stop_rx, cmd_rx))
            .expect("Failed to spawn waveform_inspector thread");
        Self {
            join: Some(join),
            stop: Some(stop_tx),
            cmd: Some(cmd_tx),
        }
    }

    fn send(&self, cmd: ViewCommand) {
        if let Some(tx) = &self.cmd {
            let _ = tx.send(cmd);
        }
    }
}

impl Drop for WaveformViewer {
    fn drop(&mut self) {
        // Drop the senders; the thread sees `Disconnected` on the stop
        // channel and exits.
        self.cmd = None;
        self.stop = None;
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Display state built once on spawn. Holds the raw mono per-channel
/// buffers (L=0, R=1); the per-pixel envelopes are derived in
/// [`EnvelopeCache`] which is rebuilt whenever the window width
/// changes.
struct ViewerState {
    expected: [Vec<f32>; 2],
    actual: [Vec<f32>; 2],
    total_samples: usize,
    duration: Duration,
    peak: f32,
    /// Sample-index ranges where consecutive chunks are at least
    /// [`GAP_THRESHOLD`] apart on the timeline. `[0]` = expected,
    /// `[1]` = actual.
    gaps: [Vec<(usize, usize)>; 2],
    /// Sample-index ranges where the detector flagged a likely
    /// discontinuity (step / kink). Computed per chunk so chunk
    /// boundaries don't trigger false positives. Channels (L, R)
    /// merged per side. `[0]` = expected, `[1]` = actual.
    artifacts: [Vec<(usize, usize)>; 2],
    /// Mono `(L+R)/2` mixdown for FFT-based views. Stored alongside
    /// per-channel buffers since we don't want to re-mix on every
    /// cursor-driven spectrum redraw.
    mono_expected: Vec<f32>,
    mono_actual: Vec<f32>,
    /// Frequency (Hz) of the strongest bin per STFT frame, mono per
    /// side. Frame `f` covers samples centred on
    /// `f * STFT_HOP + STFT_WINDOW/2`.
    primary_freq_expected: Vec<f32>,
    primary_freq_actual: Vec<f32>,
}

/// Per-pixel-column envelopes for all six lanes at a given width and
/// visible sample range. The 6 lanes share one (width, view_start,
/// view_end) tuple, so scroll zoom changes them all together.
struct EnvelopeCache {
    width: usize,
    view_start: usize,
    view_end: usize,
    expected: [Envelope; 2],
    actual: [Envelope; 2],
}

impl EnvelopeCache {
    fn compute(state: &ViewerState, width: usize, view_start: usize, view_end: usize) -> Self {
        Self {
            width,
            view_start,
            view_end,
            expected: [
                Envelope::compute(&state.expected[CH_L], view_start, view_end, width),
                Envelope::compute(&state.expected[CH_R], view_start, view_end, width),
            ],
            actual: [
                Envelope::compute(&state.actual[CH_L], view_start, view_end, width),
                Envelope::compute(&state.actual[CH_R], view_start, view_end, width),
            ],
        }
    }

    fn matches(&self, width: usize, view_start: usize, view_end: usize) -> bool {
        self.width == width && self.view_start == view_start && self.view_end == view_end
    }
}

/// Per-pixel-column min/max envelope for one channel over the full
/// duration. Used to draw the overview lanes without rescanning the
/// raw samples on every frame.
struct Envelope {
    min: Vec<f32>,
    max: Vec<f32>,
}

impl ViewerState {
    fn build(expected_chunks: Vec<AudioSampleBatch>, actual_chunks: Vec<AudioSampleBatch>) -> Self {
        let gaps = [compute_gaps(&expected_chunks), compute_gaps(&actual_chunks)];
        let expected = chunks_to_stereo(&expected_chunks);
        let actual = chunks_to_stereo(&actual_chunks);
        let total_samples = expected[CH_L]
            .len()
            .max(expected[CH_R].len())
            .max(actual[CH_L].len())
            .max(actual[CH_R].len());
        let peak = [
            peak_abs(&expected[CH_L]),
            peak_abs(&expected[CH_R]),
            peak_abs(&actual[CH_L]),
            peak_abs(&actual[CH_R]),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max)
        .max(1.0);
        let duration = Duration::from_secs_f64(total_samples as f64 / SAMPLE_RATE as f64);
        let artifacts = [
            detect_artifacts(&expected_chunks, peak),
            detect_artifacts(&actual_chunks, peak),
        ];
        let mono_expected = mix_to_mono(&expected[CH_L], &expected[CH_R]);
        let mono_actual = mix_to_mono(&actual[CH_L], &actual[CH_R]);
        let primary_freq_expected = compute_primary_freq_series(&mono_expected);
        let primary_freq_actual = compute_primary_freq_series(&mono_actual);
        Self {
            expected,
            actual,
            total_samples,
            duration,
            peak,
            gaps,
            artifacts,
            mono_expected,
            mono_actual,
            primary_freq_expected,
            primary_freq_actual,
        }
    }
}

fn mix_to_mono(left: &[f32], right: &[f32]) -> Vec<f32> {
    let n = left.len().max(right.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let l = left.get(i).copied().unwrap_or(0.0);
        let r = right.get(i).copied().unwrap_or(0.0);
        out.push((l + r) * 0.5);
    }
    out
}

/// Run an STFT over `samples` with a Hann window, returning the
/// frequency of the strongest bin per frame. Failed FFTs (vanishingly
/// rare with a fixed power-of-two window) yield 0 Hz.
fn compute_primary_freq_series(samples: &[f32]) -> Vec<f32> {
    let mut out = Vec::new();
    if samples.len() < STFT_WINDOW {
        return out;
    }
    let mut start = 0;
    while start + STFT_WINDOW <= samples.len() {
        let win = hann_window(&samples[start..start + STFT_WINDOW]);
        let freq = match samples_fft_to_spectrum(
            &win,
            SAMPLE_RATE,
            FrequencyLimit::All,
            Some(&divide_by_N),
        ) {
            Ok(spec) => primary_frequency(&spec),
            Err(_) => 0.0,
        };
        out.push(freq);
        start += STFT_HOP;
    }
    out
}

/// Frequency of the bin with the largest magnitude. Returns 0 when
/// the spectrum is effectively silent.
fn primary_frequency(spec: &FrequencySpectrum) -> f32 {
    let mut best_freq = 0.0_f32;
    let mut best_val = 0.0_f32;
    for (freq, val) in spec.data() {
        let v = val.val();
        if v > best_val {
            best_val = v;
            best_freq = freq.val();
        }
    }
    if best_val > 0.0 { best_freq } else { 0.0 }
}

/// Run the discontinuity detector on each chunk independently (so
/// chunk-boundary zeros from `chunks_to_stereo` don't generate false
/// positives), once per channel, then merge all flagged intervals on
/// the global timeline.
fn detect_artifacts(chunks: &[AudioSampleBatch], peak: f32) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    let mut chunk_l: Vec<f32> = Vec::new();
    let mut chunk_r: Vec<f32> = Vec::new();
    for c in chunks {
        let frames = c.samples.len() / 2;
        chunk_l.clear();
        chunk_r.clear();
        chunk_l.reserve(frames);
        chunk_r.reserve(frames);
        for pair in c.samples.chunks_exact(2) {
            chunk_l.push(pair[0]);
            chunk_r.push(pair[1]);
        }
        let start = pts_to_sample(c.pts);
        for (s, e) in detect_artifacts_one(&chunk_l, peak) {
            all.push((start + s, start + e));
        }
        for (s, e) in detect_artifacts_one(&chunk_r, peak) {
            all.push((start + s, start + e));
        }
    }
    all.sort_by_key(|x| x.0);
    merge_overlapping(all)
}

/// Detector for a single contiguous mono buffer. Flags samples where
/// `|d1|` exceeds its sliding-window mean by a configured multiple
/// AND clears an absolute floor. Returns `[start, end)` sample-index
/// intervals on the input's local coordinate system.
fn detect_artifacts_one(samples: &[f32], peak: f32) -> Vec<(usize, usize)> {
    let n = samples.len();
    if n < 2 {
        return Vec::new();
    }
    let mut d1 = vec![0.0_f32; n];
    for i in 1..n {
        d1[i] = (samples[i] - samples[i - 1]).abs();
    }
    let mean_d1 = sliding_mean(&d1, ARTIFACT_WINDOW_RADIUS);
    let floor_d1 = peak * ARTIFACT_D1_FLOOR_FRAC;
    let mut flagged = vec![false; n];
    for i in 1..n {
        if d1[i] > floor_d1 && d1[i] > ARTIFACT_D1_MULT * mean_d1[i].max(1.0) {
            flagged[i] = true;
        }
    }
    intervals_from_flags(&flagged, ARTIFACT_MERGE_GAP)
}

fn sliding_mean(values: &[f32], radius: usize) -> Vec<f32> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0_f32; n];
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    for i in 0..=radius.min(n - 1) {
        sum += values[i] as f64;
        count += 1;
    }
    out[0] = (sum / count as f64) as f32;
    for i in 1..n {
        if i + radius < n {
            sum += values[i + radius] as f64;
            count += 1;
        }
        if let Some(rem) = i.checked_sub(radius + 1) {
            sum -= values[rem] as f64;
            count = count.saturating_sub(1);
        }
        out[i] = if count > 0 {
            (sum / count as f64) as f32
        } else {
            0.0
        };
    }
    out
}

/// Walk a `flagged` boolean array and produce contiguous intervals.
/// Adjacent flagged regions separated by fewer than `merge_gap`
/// non-flagged samples are coalesced into one interval.
fn intervals_from_flags(flagged: &[bool], merge_gap: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let n = flagged.len();
    let mut i = 0;
    while i < n {
        if !flagged[i] {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        i += 1;
        while i < n {
            if flagged[i] {
                end = i + 1;
                i += 1;
            } else {
                let bound = (i + merge_gap).min(n);
                if (i..bound).any(|k| flagged[k]) {
                    i += 1;
                } else {
                    break;
                }
            }
        }
        out.push((start, end));
    }
    out
}

fn merge_overlapping(intervals: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in intervals {
        if let Some(last) = out.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        out.push((s, e));
    }
    out
}

/// Walk the chunk list (assumed in pts order) and return the
/// `[start_sample, end_sample)` ranges where the next chunk starts at
/// least [`GAP_THRESHOLD`] after the previous one ended. Overlaps and
/// sub-threshold jitter are ignored.
fn compute_gaps(chunks: &[AudioSampleBatch]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for pair in chunks.windows(2) {
        let prev_dur =
            Duration::from_secs_f64((pair[0].samples.len() / 2) as f64 / SAMPLE_RATE as f64);
        let prev_end = pair[0].pts + prev_dur;
        let next_start = pair[1].pts;
        if next_start <= prev_end {
            continue;
        }
        if next_start - prev_end < GAP_THRESHOLD {
            continue;
        }
        out.push((pts_to_sample(prev_end), pts_to_sample(next_start)));
    }
    out
}

impl Envelope {
    /// Compute a min/max envelope of `samples[view_start..view_end]`
    /// distributed across `width` pixel columns.
    fn compute(samples: &[f32], view_start: usize, view_end: usize, width: usize) -> Self {
        let mut min = vec![0.0_f32; width];
        let mut max = vec![0.0_f32; width];
        let view_len = view_end.saturating_sub(view_start).max(1) as f64;
        for col in 0..width {
            let off_start = (col as f64 * view_len / width as f64) as usize;
            let off_end = ((col + 1) as f64 * view_len / width as f64) as usize;
            let start = (view_start + off_start).min(samples.len()).min(view_end);
            let end = (view_start + off_end).min(samples.len()).min(view_end);
            if start >= end {
                continue;
            }
            let mut mn = 0.0_f32;
            let mut mx = 0.0_f32;
            for &s in &samples[start..end] {
                if s < mn {
                    mn = s;
                }
                if s > mx {
                    mx = s;
                }
            }
            min[col] = mn;
            max[col] = mx;
        }
        Self { min, max }
    }
}

fn peak_abs(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max)
}

/// Demultiplex interleaved stereo chunks into two contiguous mono
/// buffers indexed by sample number — `[L, R]`. Each chunk's samples
/// are placed starting at `pts * SAMPLE_RATE`, so gaps in the input
/// become silence and reordered chunks still land at the right place.
fn chunks_to_stereo(chunks: &[AudioSampleBatch]) -> [Vec<f32>; 2] {
    if chunks.is_empty() {
        return [Vec::new(), Vec::new()];
    }
    let mut max_end_sample = 0_usize;
    for c in chunks {
        let start = pts_to_sample(c.pts);
        let end = start + c.samples.len() / 2;
        max_end_sample = max_end_sample.max(end);
    }
    let mut l = vec![0.0_f32; max_end_sample];
    let mut r = vec![0.0_f32; max_end_sample];
    for c in chunks {
        let start = pts_to_sample(c.pts);
        for (i, pair) in c.samples.chunks_exact(2).enumerate() {
            let idx = start + i;
            if idx < l.len() {
                l[idx] = pair[0];
                r[idx] = pair[1];
            }
        }
    }
    [l, r]
}

fn pts_to_sample(pts: Duration) -> usize {
    (pts.as_secs_f64() * SAMPLE_RATE as f64) as usize
}

/// Minimum visible window in samples — guards against runaway zoom-in.
/// At 48 kHz this is roughly 1.3 ms.
const MIN_VIEW_SAMPLES: usize = 64;

/// Multiplicative zoom factor applied per unit of scroll-wheel Y
/// delta. < 1 zooms in (positive scroll), > 1 zooms out. Closer to
/// 1.0 = slower zoom per tick.
const ZOOM_PER_TICK: f64 = 0.95;

/// Apply a scroll-wheel zoom to `[view_start, view_end]`, pivoting
/// around `cursor_x` (in pixel space) so the time under the cursor
/// stays anchored. Clamps to `[0, total]` and `MIN_VIEW_SAMPLES`.
fn apply_zoom(
    view_start: &mut usize,
    view_end: &mut usize,
    total: usize,
    width: usize,
    cursor_x: usize,
    scroll_y: f32,
) {
    if width == 0 || total == 0 || scroll_y == 0.0 {
        return;
    }
    let view_len = (*view_end - *view_start).max(1) as f64;
    let factor = ZOOM_PER_TICK.powf(-scroll_y as f64);
    let new_len = (view_len * factor)
        .round()
        .clamp(MIN_VIEW_SAMPLES as f64, total as f64);
    let pivot_frac = cursor_x as f64 / width as f64;
    let pivot_sample = *view_start as f64 + pivot_frac * view_len;
    let mut new_start = (pivot_sample - pivot_frac * new_len).max(0.0);
    let mut new_end = new_start + new_len;
    if new_end > total as f64 {
        new_end = total as f64;
        new_start = (new_end - new_len).max(0.0);
    }
    *view_start = new_start as usize;
    *view_end = (new_end as usize).max(*view_start + MIN_VIEW_SAMPLES.min(total));
    if *view_end > total {
        *view_end = total;
    }
}

fn run_window(state: ViewerState, stop_rx: Receiver<()>, cmd_rx: Receiver<ViewCommand>) {
    let opts = WindowOptions {
        resize: true,
        scale_mode: ScaleMode::UpperLeft,
        ..WindowOptions::default()
    };
    let mut window = match Window::new(
        "waveform_inspector — scroll: zoom  ←/→: pan  r: reset zoom  Esc: close",
        INIT_W,
        CANVAS_H,
        opts,
    ) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create waveform_inspector window: {e}");
            return;
        }
    };
    window.set_target_fps(60);

    let (mut w, mut h) = window.get_size();
    w = w.max(MIN_W);
    h = h.max(1);
    let mut canvas = vec![BG; w * h];
    let mut view_start: usize = 0;
    let mut view_end: usize = state.total_samples.max(1);
    let mut env = EnvelopeCache::compute(&state, w, view_start, view_end);

    loop {
        if matches!(stop_rx.try_recv(), Err(TryRecvError::Disconnected)) {
            break;
        }
        if !window.is_open() || window.is_key_down(Key::Escape) {
            break;
        }

        // Drain any view commands sent from the inquire menu thread.
        let total = state.total_samples.max(1);
        let one_sec = (SAMPLE_RATE as usize).min(total).max(MIN_VIEW_SAMPLES);
        loop {
            match cmd_rx.try_recv() {
                Ok(ViewCommand::ShowFull) => {
                    view_start = 0;
                    view_end = total;
                }
                Ok(ViewCommand::ShowOneSecond) => {
                    if view_end - view_start == one_sec {
                        // Already a 1-sec window — advance to the next.
                        let new_start = view_end.min(total.saturating_sub(one_sec));
                        view_start = new_start;
                        view_end = (new_start + one_sec).min(total);
                    } else {
                        // Not in 1-sec mode — snap to the first second.
                        view_start = 0;
                        view_end = one_sec;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        let (cur_w, cur_h) = window.get_size();
        let cur_w = cur_w.max(MIN_W);
        let cur_h = cur_h.max(1);
        if cur_w != w || cur_h != h {
            w = cur_w;
            h = cur_h;
            canvas = vec![BG; w * h];
        }

        let mouse_x = window
            .get_mouse_pos(MouseMode::Clamp)
            .map(|(x, _)| (x as usize).min(w - 1));

        // Mouse-wheel zoom on the envelope lanes. Pivots around the
        // cursor X so the time under the mouse stays put while the
        // surrounding range stretches/contracts.
        if let (Some(cursor_x), Some((_, scroll_y))) = (mouse_x, window.get_scroll_wheel()) {
            apply_zoom(
                &mut view_start,
                &mut view_end,
                state.total_samples.max(1),
                w,
                cursor_x,
                scroll_y,
            );
        }
        // Reset zoom on `r`.
        if window.is_key_down(Key::R) {
            view_start = 0;
            view_end = state.total_samples.max(1);
        }
        // Pan when zoomed in: hold left / right to scroll the view.
        // Per-frame shift is 2% of the visible range, so at 60 fps a
        // full view-width takes ~0.8 s to pan.
        let total = state.total_samples.max(1);
        let view_len = view_end.saturating_sub(view_start);
        let pan = (view_len / 50).max(1);
        if window.is_key_down(Key::Left) {
            let shift = pan.min(view_start);
            view_start -= shift;
            view_end -= shift;
        }
        if window.is_key_down(Key::Right) {
            let shift = pan.min(total.saturating_sub(view_end));
            view_start += shift;
            view_end += shift;
        }

        if !env.matches(w, view_start, view_end) {
            env = EnvelopeCache::compute(&state, w, view_start, view_end);
        }

        let cursor_t = mouse_x.map(|x| {
            let view_len = view_end.saturating_sub(view_start) as f64;
            let frac = x as f64 / w.max(1) as f64;
            (view_start as f64 + frac * view_len) / SAMPLE_RATE as f64
        });

        for px in canvas.iter_mut() {
            *px = BG;
        }
        draw_header(&mut canvas, w, h, &state, view_start, view_end);

        let lanes: [(&str, &Envelope, u32); NUM_LANES] = [
            ("expected L", &env.expected[CH_L], COLOR_EXPECTED),
            ("expected R", &env.expected[CH_R], COLOR_EXPECTED),
            ("actual L", &env.actual[CH_L], COLOR_ACTUAL),
            ("actual R", &env.actual[CH_R], COLOR_ACTUAL),
        ];
        let lanes_top = HEADER_H;
        for (i, (label, lane_env, color)) in lanes.iter().enumerate() {
            let top = lanes_top + i * (LANE_H + LANE_GAP);
            draw_envelope_lane(&mut canvas, w, h, label, lane_env, top, *color, state.peak);
        }
        let env_lanes_bottom = lanes_top + NUM_LANES * LANE_H + (NUM_LANES - 1) * LANE_GAP;

        let gap_lanes_top = env_lanes_bottom + LANE_GAP;
        let gap_thresh_us = GAP_THRESHOLD.as_micros();
        let gap_lane_specs: [(String, &[(usize, usize)]); NUM_GAP_LANES] = [
            (
                format!(
                    "gaps expected (≥{gap_thresh_us}µs, n={})",
                    state.gaps[0].len()
                ),
                &state.gaps[0],
            ),
            (
                format!(
                    "gaps actual (≥{gap_thresh_us}µs, n={})",
                    state.gaps[1].len()
                ),
                &state.gaps[1],
            ),
        ];
        for (i, (label, gaps)) in gap_lane_specs.iter().enumerate() {
            let top = gap_lanes_top + i * (GAP_LANE_H + LANE_GAP);
            draw_interval_lane(
                &mut canvas,
                w,
                h,
                label,
                gaps,
                COLOR_GAP,
                top,
                view_start,
                view_end,
            );
        }
        let gap_lanes_bottom =
            gap_lanes_top + NUM_GAP_LANES * GAP_LANE_H + (NUM_GAP_LANES - 1) * LANE_GAP;

        let artifact_lanes_top = gap_lanes_bottom + LANE_GAP;
        let artifact_lane_specs: [(String, &[(usize, usize)]); NUM_ARTIFACT_LANES] = [
            (
                format!("artifacts expected (n={})", state.artifacts[0].len()),
                &state.artifacts[0],
            ),
            (
                format!("artifacts actual (n={})", state.artifacts[1].len()),
                &state.artifacts[1],
            ),
        ];
        for (i, (label, intervals)) in artifact_lane_specs.iter().enumerate() {
            let top = artifact_lanes_top + i * (GAP_LANE_H + LANE_GAP);
            draw_interval_lane(
                &mut canvas,
                w,
                h,
                label,
                intervals,
                COLOR_ARTIFACT,
                top,
                view_start,
                view_end,
            );
        }
        let artifact_lanes_bottom = artifact_lanes_top
            + NUM_ARTIFACT_LANES * GAP_LANE_H
            + (NUM_ARTIFACT_LANES - 1) * LANE_GAP;

        let primary_freq_top = artifact_lanes_bottom + LANE_GAP;
        draw_primary_freq_lane(
            &mut canvas,
            w,
            h,
            "primary frequency (argmax bin, log 20Hz..fs/2)",
            &state.primary_freq_expected,
            &state.primary_freq_actual,
            cursor_t,
            primary_freq_top,
            view_start,
            view_end,
        );
        let primary_freq_bottom = primary_freq_top + PRIMARY_FREQ_LANE_H;

        if let Some(x) = mouse_x {
            draw_cursor(&mut canvas, w, h, x, lanes_top, primary_freq_bottom);
        }

        let zoom_l_top = primary_freq_bottom + LANE_GAP;
        let zoom_r_top = zoom_l_top + ZOOM_H + LANE_GAP;
        draw_zoom_strip(
            &mut canvas,
            w,
            h,
            cursor_t,
            zoom_l_top,
            "zoom L",
            &state.expected[CH_L],
            &state.actual[CH_L],
            &state,
        );
        draw_zoom_strip(
            &mut canvas,
            w,
            h,
            cursor_t,
            zoom_r_top,
            "zoom R",
            &state.expected[CH_R],
            &state.actual[CH_R],
            &state,
        );

        let spectrum_top = zoom_r_top + ZOOM_H + LANE_GAP;
        draw_spectrum_strip(
            &mut canvas,
            w,
            h,
            cursor_t,
            spectrum_top,
            "spectrum",
            &state,
        );

        if let Err(e) = window.update_with_buffer(&canvas, w, h) {
            error!("waveform_inspector update failed: {e}");
            break;
        }
    }
}

fn draw_header(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    state: &ViewerState,
    view_start: usize,
    view_end: usize,
) {
    let view_start_t = view_start as f64 / SAMPLE_RATE as f64;
    let view_end_t = view_end as f64 / SAMPLE_RATE as f64;
    let line = format!(
        "waveform_inspector — duration={:.3}s  peak={:.0}  view=[{:.3}s, {:.3}s] ({:.3}s)",
        state.duration.as_secs_f64(),
        state.peak,
        view_start_t,
        view_end_t,
        view_end_t - view_start_t,
    );
    draw_text(canvas, w, h, 6, 6, &line, TEXT);
}

fn draw_envelope_lane(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    label: &str,
    env: &Envelope,
    top: usize,
    color: u32,
    peak: f32,
) {
    let plot_top = top + LABEL_BAND_H;
    let plot_bot = top + LANE_H - 2;
    let center = (plot_top + plot_bot) / 2;
    let half = (plot_bot - plot_top) as f32 / 2.0;
    let scale = if peak > 0.0 { half / peak } else { 0.0 };
    if center < h {
        for x in 0..w {
            canvas[center * w + x] = AXIS;
        }
    }
    let cols = w.min(env.min.len());
    for col in 0..cols {
        let mn = env.min[col];
        let mx = env.max[col];
        if mn == 0.0 && mx == 0.0 {
            continue;
        }
        let y_top = clamp_y(center as f32 - mx * scale, plot_top, plot_bot);
        let y_bot = clamp_y(center as f32 - mn * scale, plot_top, plot_bot);
        for y in y_top..=y_bot.min(h.saturating_sub(1)) {
            canvas[y * w + col] = color;
        }
    }
    draw_text(canvas, w, h, 6, top + 2, label, TEXT);
}

/// Render a thin "marker" lane: a track marking sample-index ranges
/// in `intervals` as filled `color` rectangles on the timeline. Used
/// for both gap lanes (`COLOR_GAP`) and artifact lanes
/// (`COLOR_ARTIFACT`). Intervals fully outside the current view are
/// skipped, partial overlaps are clipped.
fn draw_interval_lane(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    label: &str,
    intervals: &[(usize, usize)],
    color: u32,
    top: usize,
    view_start: usize,
    view_end: usize,
) {
    let plot_top = top + LABEL_BAND_H;
    let plot_bot = top + GAP_LANE_H - 2;
    // Faint axis line so an empty lane still looks like a track.
    let center = (plot_top + plot_bot) / 2;
    if center < h {
        for x in 0..w {
            canvas[center * w + x] = AXIS;
        }
    }
    let view_len = view_end.saturating_sub(view_start).max(1) as f64;
    for &(gs, ge) in intervals {
        if ge <= view_start || gs >= view_end {
            continue;
        }
        let cs = gs.max(view_start);
        let ce = ge.min(view_end);
        let x_start = ((cs - view_start) as f64 / view_len * w as f64) as usize;
        let mut x_end = ((ce - view_start) as f64 / view_len * w as f64).ceil() as usize;
        x_end = x_end.min(w).max(x_start + 1);
        for x in x_start..x_end {
            for y in plot_top..plot_bot.min(h) {
                canvas[y * w + x] = color;
            }
        }
    }
    draw_text(canvas, w, h, 6, top + 2, label, TEXT);
}

fn draw_cursor(canvas: &mut [u32], w: usize, h: usize, x: usize, top: usize, bottom: usize) {
    if x >= w {
        return;
    }
    for y in top..bottom.min(h) {
        canvas[y * w + x] = CURSOR;
    }
}

/// Plot expected and actual sample values around the cursor for a
/// single channel, overlaid in their respective colours so phase /
/// sample-level differences are directly visible. `cursor_t` is in
/// seconds and already accounts for the current envelope zoom.
fn draw_zoom_strip(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    cursor_t: Option<f64>,
    top: usize,
    label: &str,
    expected: &[f32],
    actual: &[f32],
    state: &ViewerState,
) {
    let plot_top = top + LABEL_BAND_H;
    let plot_bot = top + ZOOM_H - 2;
    let center = (plot_top + plot_bot) / 2;
    if center < h {
        for x in 0..w {
            canvas[center * w + x] = AXIS;
        }
    }

    let Some(cursor_t) = cursor_t else {
        let hint = format!("{label} — hover the timeline");
        draw_text(canvas, w, h, 6, top + 2, &hint, TEXT);
        return;
    };

    let start_t = (cursor_t - ZOOM_HALF_WINDOW as f64).max(0.0);
    let end_t = (cursor_t + ZOOM_HALF_WINDOW as f64).min(state.duration.as_secs_f64());
    let start_s = (start_t * SAMPLE_RATE as f64) as usize;
    let end_s = (end_t * SAMPLE_RATE as f64) as usize;
    let span = end_s.saturating_sub(start_s);
    if span == 0 {
        let hint = format!("{label} @ t={cursor_t:.4}s — out of range");
        draw_text(canvas, w, h, 6, top + 2, &hint, TEXT);
        return;
    }

    let half = (plot_bot - plot_top) as f32 / 2.0;
    let scale = if state.peak > 0.0 {
        half / state.peak
    } else {
        0.0
    };
    let mut prev: Option<(i32, i32)> = None;
    for col in 0..w {
        let s = start_s + col * span / w;
        if s >= state.total_samples {
            break;
        }
        let e = expected.get(s).copied().unwrap_or(0.0);
        let a = actual.get(s).copied().unwrap_or(0.0);
        let ye = clamp_y(center as f32 - e * scale, plot_top, plot_bot) as i32;
        let ya = clamp_y(center as f32 - a * scale, plot_top, plot_bot) as i32;
        if let Some((pe, pa)) = prev {
            draw_vline(canvas, w, h, col as i32, pe, ye, COLOR_EXPECTED);
            draw_vline(canvas, w, h, col as i32, pa, ya, COLOR_ACTUAL);
        }
        if (ye as usize) < h {
            canvas[ye as usize * w + col] = COLOR_EXPECTED;
        }
        if (ya as usize) < h {
            canvas[ya as usize * w + col] = COLOR_ACTUAL;
        }
        prev = Some((ye, ya));
    }

    // Cursor line at the centre of the strip — stays inside the plot
    // band so it doesn't bleed into the label.
    let cursor_col = w / 2;
    for y in plot_top..plot_bot.min(h) {
        canvas[y * w + cursor_col] = CURSOR;
    }

    let line = format!(
        "{label} @ t={cursor_t:.4}s  ±{:.0}ms  expected=green  actual=blue",
        ZOOM_HALF_WINDOW * 1000.0,
    );
    draw_text(canvas, w, h, 6, top + 2, &line, TEXT);
}

/// Render the primary-frequency lane: two lines (expected / actual)
/// over the full visible time range showing the frequency of the
/// strongest STFT bin at each frame. Y axis is linear 0 → fs/2.
fn draw_primary_freq_lane(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    label: &str,
    primary_freq_expected: &[f32],
    primary_freq_actual: &[f32],
    cursor_t: Option<f64>,
    top: usize,
    view_start: usize,
    view_end: usize,
) {
    let plot_top = top + LABEL_BAND_H;
    let plot_bot = top + PRIMARY_FREQ_LANE_H - 2;
    // Faint baseline at the bottom (0 Hz).
    if plot_bot < h {
        for x in 0..w {
            canvas[plot_bot * w + x] = AXIS;
        }
    }
    let min_freq = 20.0_f32;
    let max_freq = SAMPLE_RATE as f32 / 2.0;
    let log_min = min_freq.log10();
    let log_span = max_freq.log10() - log_min;
    let plot_h = (plot_bot - plot_top) as f32;
    let scale_y = |freq: f32| -> i32 {
        let f = freq.max(min_freq);
        let frac = ((f.log10() - log_min) / log_span).clamp(0.0, 1.0);
        plot_bot as i32 - (plot_h * frac).round() as i32
    };

    let view_len = view_end.saturating_sub(view_start) as f64;
    let center_offset = STFT_WINDOW as f64 / 2.0;

    let plot_series = |canvas: &mut [u32], series: &[f32], color: u32| {
        if series.is_empty() {
            return;
        }
        let mut prev: Option<i32> = None;
        for col in 0..w {
            let frac = col as f64 / w.max(1) as f64;
            let sample_at_col = view_start as f64 + frac * view_len;
            let frame_f = (sample_at_col - center_offset) / STFT_HOP as f64;
            let frame = if frame_f < 0.0 { 0 } else { frame_f as usize };
            let Some(&freq) = series.get(frame) else {
                continue;
            };
            let y = scale_y(freq);
            if let Some(py) = prev {
                draw_vline(canvas, w, h, col as i32, py, y, color);
            }
            if y >= 0 && (y as usize) < h {
                canvas[y as usize * w + col] = color;
            }
            prev = Some(y);
        }
    };
    plot_series(canvas, primary_freq_expected, COLOR_EXPECTED);
    plot_series(canvas, primary_freq_actual, COLOR_ACTUAL);

    let label_with_cursor = match cursor_t {
        Some(t) => {
            let cursor_sample = (t * SAMPLE_RATE as f64) as f64;
            let frame_f = (cursor_sample - center_offset) / STFT_HOP as f64;
            let frame = if frame_f < 0.0 { 0 } else { frame_f as usize };
            let exp = primary_freq_expected
                .get(frame)
                .map(|f| format!("{f:.0}Hz"))
                .unwrap_or_else(|| "—".to_string());
            let act = primary_freq_actual
                .get(frame)
                .map(|f| format!("{f:.0}Hz"))
                .unwrap_or_else(|| "—".to_string());
            format!("{label}  cursor: expected={exp} actual={act}")
        }
        None => label.to_string(),
    };
    draw_text(canvas, w, h, 6, top + 2, &label_with_cursor, TEXT);
}

/// Cursor-driven instantaneous magnitude spectrum. Takes
/// `STFT_WINDOW` mono samples centred on the cursor, runs the same
/// Hann + FFT that the centroid uses, and overlays expected (green)
/// vs actual (blue) on a 0 → fs/2 frequency axis (linear) with a
/// `SPECTRUM_DB_FLOOR` → 0 dB y axis (relative to the per-frame peak
/// across both streams, so a quiet cursor moment isn't flat).
fn draw_spectrum_strip(
    canvas: &mut [u32],
    w: usize,
    h: usize,
    cursor_t: Option<f64>,
    top: usize,
    label: &str,
    state: &ViewerState,
) {
    let plot_top = top + LABEL_BAND_H;
    let plot_bot = top + SPECTRUM_H - 2;
    if plot_bot < h {
        for x in 0..w {
            canvas[plot_bot * w + x] = AXIS;
        }
    }

    let Some(cursor_t) = cursor_t else {
        let hint = format!("{label} — hover the timeline");
        draw_text(canvas, w, h, 6, top + 2, &hint, TEXT);
        return;
    };

    let cursor_s = (cursor_t * SAMPLE_RATE as f64) as usize;
    let half = STFT_WINDOW / 2;
    let fits = |buf: &[f32]| cursor_s >= half && cursor_s + half <= buf.len();
    let exp_spec = if fits(&state.mono_expected) {
        let start = cursor_s - half;
        let win = hann_window(&state.mono_expected[start..start + STFT_WINDOW]);
        samples_fft_to_spectrum(&win, SAMPLE_RATE, FrequencyLimit::All, Some(&divide_by_N)).ok()
    } else {
        None
    };
    let act_spec = if fits(&state.mono_actual) {
        let start = cursor_s - half;
        let win = hann_window(&state.mono_actual[start..start + STFT_WINDOW]);
        samples_fft_to_spectrum(&win, SAMPLE_RATE, FrequencyLimit::All, Some(&divide_by_N)).ok()
    } else {
        None
    };
    if exp_spec.is_none() && act_spec.is_none() {
        let hint = format!("{label} @ t={cursor_t:.4}s — out of range");
        draw_text(canvas, w, h, 6, top + 2, &hint, TEXT);
        return;
    }
    let exp_data = exp_spec.as_ref().map(|s| s.data());
    let act_data = act_spec.as_ref().map(|s| s.data());
    let bins = exp_data
        .map(|d| d.len())
        .into_iter()
        .chain(act_data.map(|d| d.len()))
        .min()
        .unwrap_or(0)
        .max(1);

    // Per-frame peak as the 0 dB reference so quiet moments still have
    // visible structure. Computed across whichever sides are available.
    let peak_of = |d: Option<
        &[(
            spectrum_analyzer::Frequency,
            spectrum_analyzer::FrequencyValue,
        )],
    >| {
        d.map(|d| d.iter().map(|(_, v)| v.val()).fold(0.0_f32, f32::max))
            .unwrap_or(0.0)
    };
    let ref_val = peak_of(exp_data).max(peak_of(act_data)).max(1e-6);
    let to_db = |v: f32| -> f32 {
        let r = (v / ref_val).max(1e-6);
        20.0 * r.log10()
    };
    let plot_h = (plot_bot - plot_top) as f32;
    let scale_y = |db: f32| -> i32 {
        let span = -SPECTRUM_DB_FLOOR;
        let frac = ((db - SPECTRUM_DB_FLOOR) / span).clamp(0.0, 1.0);
        plot_bot as i32 - (plot_h * frac).round() as i32
    };

    let mut prev_e: Option<i32> = None;
    let mut prev_a: Option<i32> = None;
    for col in 0..w {
        let bin_idx = ((col * bins) / w.max(1)).min(bins - 1);
        let ye = exp_data.map(|d| scale_y(to_db(d[bin_idx].1.val())));
        let ya = act_data.map(|d| scale_y(to_db(d[bin_idx].1.val())));
        if let (Some(py), Some(y)) = (prev_e, ye) {
            draw_vline(canvas, w, h, col as i32, py, y, COLOR_EXPECTED);
        }
        if let (Some(py), Some(y)) = (prev_a, ya) {
            draw_vline(canvas, w, h, col as i32, py, y, COLOR_ACTUAL);
        }
        if let Some(y) = ye {
            if y >= 0 && (y as usize) < h {
                canvas[y as usize * w + col] = COLOR_EXPECTED;
            }
        }
        if let Some(y) = ya {
            if y >= 0 && (y as usize) < h {
                canvas[y as usize * w + col] = COLOR_ACTUAL;
            }
        }
        prev_e = ye;
        prev_a = ya;
    }

    let line = format!(
        "{label} @ t={cursor_t:.4}s  freq 0..{:.0}Hz  {}..0 dB",
        SAMPLE_RATE as f32 / 2.0,
        SPECTRUM_DB_FLOOR,
    );
    draw_text(canvas, w, h, 6, top + 2, &line, TEXT);
}

fn draw_vline(canvas: &mut [u32], w: usize, h: usize, x: i32, y0: i32, y1: i32, color: u32) {
    if x < 0 || (x as usize) >= w {
        return;
    }
    let lo = y0.min(y1).max(0);
    let hi = y0.max(y1).min(h as i32 - 1);
    if hi < 0 {
        return;
    }
    let x = x as usize;
    for y in lo..=hi {
        canvas[y as usize * w + x] = color;
    }
}

fn clamp_y(y: f32, lo: usize, hi: usize) -> usize {
    let y = y.round() as i32;
    y.clamp(lo as i32, hi as i32) as usize
}

fn draw_text(canvas: &mut [u32], w: usize, h: usize, x: usize, y: usize, text: &str, color: u32) {
    let mut cx = x;
    for ch in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row, byte) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if byte & (1 << col) == 0 {
                        continue;
                    }
                    for dy in 0..FONT_SCALE {
                        for dx in 0..FONT_SCALE {
                            let px = cx + col * FONT_SCALE + dx;
                            let py = y + row * FONT_SCALE + dy;
                            if px < w && py < h {
                                canvas[py * w + px] = color;
                            }
                        }
                    }
                }
            }
        }
        cx += GLYPH_W;
        if cx >= w {
            break;
        }
    }
}
