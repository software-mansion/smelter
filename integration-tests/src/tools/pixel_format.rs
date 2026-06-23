//! Pixel-format helpers shared by tools and binaries that inspect
//! decoded frames: YUV → RGBA conversion for display, and the
//! per-pixel frame-diff metric.

use anyhow::Result;
use smelter_render::{Frame, FrameData, YuvPlanes};

/// Convert a decoded frame to packed 8-bit RGBA. Only handles the
/// planar YUV formats the H.264 decoders in this crate ever produce.
pub fn frame_to_rgba(frame: &Frame) -> Result<Vec<u8>> {
    let planes = match &frame.data {
        FrameData::PlanarYuv420(p)
        | FrameData::PlanarYuv422(p)
        | FrameData::PlanarYuv444(p)
        | FrameData::PlanarYuvJ420(p) => p,
        other => {
            anyhow::bail!("frame_to_rgba: unsupported frame format {other:?}");
        }
    };
    Ok(yuv420_to_rgba(planes, frame.resolution.width, frame.resolution.height))
}

/// BT.709 limited-range YUV → RGBA. Mirrors the conversion used by
/// the render-test snapshotting code.
pub fn yuv420_to_rgba(planes: &YuvPlanes, width: usize, height: usize) -> Vec<u8> {
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
/// formats this helper doesn't know how to compare.
pub fn mean_square_error(expected: &Frame, actual: &Frame) -> Option<f64> {
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
    let planes =
        [(&e.y_plane, &a.y_plane), (&e.u_plane, &a.u_plane), (&e.v_plane, &a.v_plane)];
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
