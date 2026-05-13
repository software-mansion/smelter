//! Side-by-side inspector for render-test PNG snapshots.
//!
//! Render tests produce a single PNG per snapshot. After a failed
//! run, the harness writes `actual_<name>.png` and `expected_<name>.png`
//! to the render-test workdir. This tool loads both, pushes them into
//! [`crate::tools::frame_inspector`], and blocks until the user closes
//! the window.
//!
//! Either side may be missing — for example a brand-new test has no
//! committed `expected`, and a test that panicked before rendering has
//! no `actual`. The viewer falls back to a black placeholder of the
//! present side's resolution so the layout still works.

use std::path::Path;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::tools::frame_inspector::{FrameInspector, Pair};

pub type InspectorHandle = FrameInspector;

/// Open the inspector with `expected` on the left and `actual` on the
/// right. Blocks the calling thread until the user closes the window
/// (Esc or window close).
pub fn run(expected: &Path, actual: &Path) -> Result<()> {
    let inspector = open(expected, actual)?;
    inspector.wait();
    Ok(())
}

/// Open the inspector without blocking. Returns the handle so the
/// caller can keep the window alive and push updates via [`refresh`].
pub fn open(expected: &Path, actual: &Path) -> Result<FrameInspector> {
    info!("render_inspector: expected = {}", expected.display());
    info!("render_inspector: actual = {}", actual.display());

    let pair = load_pair(expected, actual)?;
    let inspector = FrameInspector::spawn();
    inspector.update(pair);
    Ok(inspector)
}

/// Reload images from disk and push an update to an already-open
/// inspector. Returns `true` if the update reached the window,
/// `false` if the window was already closed or the images could not
/// be loaded.
pub fn refresh(inspector: &FrameInspector, expected: &Path, actual: &Path) -> bool {
    match load_pair(expected, actual) {
        Ok(pair) => inspector.update(pair),
        Err(e) => {
            warn!("render_inspector refresh failed: {e:#}");
            false
        }
    }
}

fn load_pair(expected: &Path, actual: &Path) -> Result<Pair> {
    let expected_img = load_optional(expected, "expected")?;
    let actual_img = load_optional(actual, "actual")?;

    if expected_img.is_none() && actual_img.is_none() {
        anyhow::bail!(
            "Neither expected ({}) nor actual ({}) snapshot exists",
            expected.display(),
            actual.display()
        );
    }

    let (left_w, left_h, right_w, right_h) = pick_dimensions(&expected_img, &actual_img);
    let left = expected_img.unwrap_or_else(|| black_image(left_w, left_h));
    let right = actual_img.unwrap_or_else(|| black_image(right_w, right_h));

    let mse = if left.width == right.width && left.height == right.height {
        Some(mean_square_error(&left.rgba, &right.rgba))
    } else {
        None
    };

    let extra_lines = |path: &Path| -> Vec<String> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // filename: {actual_|expected_}<test_name>_<pts:05>_output_1.png
        let stem = name
            .strip_prefix("actual_")
            .or_else(|| name.strip_prefix("expected_"))
            .unwrap_or(&name);
        let stem = stem.strip_suffix(".png").unwrap_or(stem);
        // split off "_output_1" then "_00000" to get test name and pts
        if let Some((rest, _output_id)) = stem.rsplit_once("_output_")
            && let Some((test_name, pts_str)) = rest.rsplit_once('_')
        {
            let pts_ms: u64 = pts_str.parse().unwrap_or(0);
            return vec![test_name.to_string(), format!("pts={pts_ms}ms")];
        }
        vec![stem.to_string()]
    };

    Ok(Pair {
        left_label: "expected".to_string(),
        left_lines: extra_lines(expected),
        left_rgba: left.rgba,
        left_w: left.width,
        left_h: left.height,
        right_label: "actual".to_string(),
        right_lines: extra_lines(actual),
        right_rgba: right.rgba,
        right_w: right.width,
        right_h: right.height,
        mse,
    })
}

struct Image {
    rgba: Vec<u8>,
    width: usize,
    height: usize,
}

fn load_optional(path: &Path, side: &str) -> Result<Option<Image>> {
    if !path.exists() {
        warn!(
            "render_inspector: {side} snapshot {} not found, will show a placeholder",
            path.display()
        );
        return Ok(None);
    }
    let img = image::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(Some(Image {
        rgba: img.into_raw(),
        width: w as usize,
        height: h as usize,
    }))
}

/// Pick canvas dimensions for each side. When one side is missing, the
/// placeholder takes the other side's dimensions so the layout looks
/// balanced.
fn pick_dimensions(left: &Option<Image>, right: &Option<Image>) -> (usize, usize, usize, usize) {
    let (lw, lh) = left.as_ref().map(|i| (i.width, i.height)).unwrap_or((0, 0));
    let (rw, rh) = right
        .as_ref()
        .map(|i| (i.width, i.height))
        .unwrap_or((0, 0));
    let fallback_w = lw.max(rw).max(1);
    let fallback_h = lh.max(rh).max(1);
    (
        if lw == 0 { fallback_w } else { lw },
        if lh == 0 { fallback_h } else { lh },
        if rw == 0 { fallback_w } else { rw },
        if rh == 0 { fallback_h } else { rh },
    )
}

fn black_image(width: usize, height: usize) -> Image {
    let mut rgba = vec![0u8; width * height * 4];
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    Image {
        rgba,
        width,
        height,
    }
}

/// Per-channel MSE matching the metric used by the render-test harness
/// (`render_tests/snapshot.rs::snapshots_diff`). Returns 0 for identical
/// buffers; saturates to a large value when sizes mismatch.
fn mean_square_error(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::INFINITY;
    }
    let mut sum_sq: u64 = 0;
    for (lhs, rhs) in a.iter().zip(b.iter()) {
        let d = *lhs as i32 - *rhs as i32;
        sum_sq += (d * d) as u64;
    }
    sum_sq as f64 / a.len() as f64
}
