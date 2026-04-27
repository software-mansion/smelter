//! Persistent side-by-side frame inspector window.
//!
//! Spawns a dedicated thread that owns a [`minifb::Window`]. The main
//! thread pushes one [`Pair`] per step and the window re-composites
//! on every frame using the current layout and mouse position.
//!
//! Intended to be shared between tools that need to diff two streams
//! of frames visually (e.g. the RTP dump inspector today, render-test
//! comparisons in the future). Labels and captions live on [`Pair`]
//! so callers can use any nomenclature — "expected"/"actual",
//! "reference"/"candidate", etc.
//!
//! ## In-window controls
//!   `1` — side-by-side layout
//!   `2` — over-under (stacked) layout
//!   `3` — slider-H (horizontal curtain wipe; mouse X moves the divider)
//!   `4` — slider-V (vertical curtain wipe; mouse Y moves the divider)
//!   `5` — toggle layout (click to swap which side is visible)
//!   `Esc` — close the window (the main thread keeps running)

use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
};

use font8x8::UnicodeFonts;
use minifb::{Key, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use tracing::{error, warn};

/// Gap (in pixels) between the two frames in the side-by-side
/// and over-under layouts.
const GAP: usize = 8;
/// Background colour for letterboxing / gaps (opaque black).
const BG: u32 = 0;
/// Slider divider colour (opaque white).
const DIVIDER: u32 = 0x00FF_FFFF;
/// Label text colour (opaque white).
const LABEL_COLOR: u32 = 0x00FF_FFFF;
/// Pixel scale for the 8×8 bitmap font.
const FONT_SCALE: usize = 2;
/// Width of one rendered glyph including its trailing inter-char gap.
const GLYPH_W: usize = 8 * FONT_SCALE + FONT_SCALE;
/// Height of one rendered glyph including its trailing gap.
const GLYPH_H: usize = 8 * FONT_SCALE + 4;
/// Worst-case width we reserve for a single per-frame label string.
const LABEL_W: usize = 32 * GLYPH_W;
/// Padding between a frame and its adjacent label area.
const LABEL_PAD: usize = 4;
/// Pixel scale for the top layout-list bar.
const BAR_SCALE: usize = 2;
/// Width of one bar-glyph including its trailing inter-char gap.
const BAR_GLYPH_W: usize = 8 * BAR_SCALE + BAR_SCALE;
/// Padding (in pixels) around the bar text on every side.
const BAR_PAD: usize = 6;
/// Height of the layout bar = one glyph row + top & bottom pad.
const LAYOUT_BAR_H: usize = 8 * BAR_SCALE + 2 * BAR_PAD;
/// Height of the MSE bar (same as layout bar).
const MSE_BAR_H: usize = LAYOUT_BAR_H;
/// Worst-case width of the layout-bar string + horizontal pad.
const BAR_TEXT_W: usize = 100 * BAR_GLYPH_W + 2 * BAR_PAD;

/// One pair of frames to display, sent from the main thread to the
/// inspector every time the caller's playhead advances.
///
/// Labels and captions are free-form strings. Labels identify each
/// side (e.g. "expected", "reference"); captions are a secondary
/// line shown under the label (e.g. "pts=1.234s" or a frame index).
/// Captions may be empty.
pub struct Pair {
    pub left_label: String,
    pub left_caption: String,
    pub left_rgba: Vec<u8>,
    pub left_w: usize,
    pub left_h: usize,
    pub right_label: String,
    pub right_caption: String,
    pub right_rgba: Vec<u8>,
    pub right_w: usize,
    pub right_h: usize,
    /// Mean square error between the two source frames, displayed in
    /// the top bar. `None` when the sides aren't comparable (different
    /// resolutions, missing side, etc.).
    pub mse: Option<f64>,
}

/// Handle to the spawned inspector thread. Dropping it closes the
/// channel, which tells the thread to exit on its next poll.
pub struct FrameInspector {
    tx: Option<Sender<Pair>>,
    join: Option<JoinHandle<()>>,
}

impl FrameInspector {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Pair>();
        let join = thread::Builder::new()
            .name("frame_inspector".into())
            .spawn(move || run(rx))
            .expect("Failed to spawn frame_inspector thread");
        Self {
            tx: Some(tx),
            join: Some(join),
        }
    }

    pub fn update(&self, pair: Pair) {
        if let Some(tx) = &self.tx {
            if tx.send(pair).is_err() {
                warn!("frame_inspector thread closed; updates will be ignored");
            }
        }
    }
}

impl Drop for FrameInspector {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    SideBySide,
    OverUnder,
    Slider,
    SliderV,
    Toggle,
}

const ALL_LAYOUTS: &[(Layout, &str)] = &[
    (Layout::SideBySide, "1 side-by-side"),
    (Layout::OverUnder, "2 over-under"),
    (Layout::Slider, "3 slider-H"),
    (Layout::SliderV, "4 slider-V"),
    (Layout::Toggle, "5 toggle"),
];

/// Pre-converted minifb-format pixel buffer plus metadata.
struct Image {
    pixels: Vec<u32>,
    width: usize,
    height: usize,
    label: String,
    caption: String,
}

impl Image {
    fn from_side(rgba: &[u8], width: usize, height: usize, label: String, caption: String) -> Self {
        Self {
            pixels: rgba_to_minifb(rgba),
            width,
            height,
            label,
            caption,
        }
    }

    fn line(&self) -> String {
        if self.caption.is_empty() {
            self.label.clone()
        } else {
            format!("{} {}", self.label, self.caption)
        }
    }
}

fn run(rx: Receiver<Pair>) {
    let first = match rx.recv() {
        Ok(p) => p,
        Err(_) => return,
    };
    let mut left = Image::from_side(
        &first.left_rgba,
        first.left_w,
        first.left_h,
        first.left_label,
        first.left_caption,
    );
    let mut right = Image::from_side(
        &first.right_rgba,
        first.right_w,
        first.right_h,
        first.right_label,
        first.right_caption,
    );
    let mut mse = first.mse;

    let max_w = left.width.max(right.width);
    let max_h = left.height.max(right.height);
    let side_by_side_w = left.width + GAP + right.width;
    let over_under_w = max_w + LABEL_PAD + LABEL_W;
    // Slider-H needs both per-side labels to sit at opposite edges of
    // a single line, so the canvas must be wide enough to fit them
    // without overlap regardless of how narrow the frames are.
    let slider_labels_w = 2 * LABEL_W + GLYPH_W;
    // Modes 3/4/5 only ever show a single frame at a time, so we want
    // them pixel-doubled by default — the canvas is sized to fit one
    // frame at 2×. Capped at 4K so absurdly large inputs stay sane.
    let single_2x = max_w * 2 <= 3840 && max_h * 2 <= 2160;
    let single_scale = if single_2x { 2 } else { 1 };
    let slider_w = single_scale * max_w;
    let slider_v_w = single_scale * max_w + LABEL_PAD + LABEL_W;
    let toggle_w = single_scale * max_w;
    let canvas_w = side_by_side_w
        .max(over_under_w)
        .max(slider_w)
        .max(slider_v_w)
        .max(toggle_w)
        .max(BAR_TEXT_W)
        .max(slider_labels_w);

    let over_under_h = left.height + GAP + right.height;
    let labels_below_h = max_h + LABEL_PAD + GLYPH_H;
    let slider_h = single_scale * max_h + LABEL_PAD + GLYPH_H;
    let slider_v_h = single_scale * max_h;
    let toggle_h = single_scale * max_h + LABEL_PAD + 2 * GLYPH_H;
    let content_h = over_under_h
        .max(labels_below_h)
        .max(slider_h)
        .max(slider_v_h)
        .max(toggle_h);
    let canvas_h = LAYOUT_BAR_H + MSE_BAR_H + content_h;

    // Tiling WMs (i3/sway/hyprland/...) ignore size hints and will
    // resize to fit the tile. `resize` accepts that; `UpperLeft`
    // anchors the canvas without scaling so extra area is just
    // background.
    let options = WindowOptions {
        resize: true,
        scale_mode: ScaleMode::UpperLeft,
        ..WindowOptions::default()
    };
    let mut window = match Window::new(initial_title(), canvas_w, canvas_h, options) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create frame_inspector window: {e}");
            drain_until_closed(rx);
            return;
        }
    };
    window.set_target_fps(60);

    let mut layout = Layout::SideBySide;
    let mut showing_right = false;
    let mut prev_mouse_down = false;
    let mut last_title = initial_title().to_string();
    let mut canvas = vec![BG; canvas_w * canvas_h];

    loop {
        if !window.is_open() || window.is_key_down(Key::Escape) {
            break;
        }

        // Drain pending pairs — keep only the latest.
        loop {
            match rx.try_recv() {
                Ok(next) => {
                    left = Image::from_side(
                        &next.left_rgba,
                        next.left_w,
                        next.left_h,
                        next.left_label,
                        next.left_caption,
                    );
                    right = Image::from_side(
                        &next.right_rgba,
                        next.right_w,
                        next.right_h,
                        next.right_label,
                        next.right_caption,
                    );
                    mse = next.mse;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if window.is_key_down(Key::Key1) {
            layout = Layout::SideBySide;
        }
        if window.is_key_down(Key::Key2) {
            layout = Layout::OverUnder;
        }
        if window.is_key_down(Key::Key3) {
            layout = Layout::Slider;
        }
        if window.is_key_down(Key::Key4) {
            layout = Layout::SliderV;
        }
        if window.is_key_down(Key::Key5) {
            layout = Layout::Toggle;
        }

        // Edge-detect a mouse click (rising edge) for the toggle
        // layout swap.
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if matches!(layout, Layout::Toggle) && mouse_down && !prev_mouse_down {
            showing_right = !showing_right;
        }
        prev_mouse_down = mouse_down;

        let desired = title_for(layout, showing_right, &left, &right);
        if desired != last_title {
            window.set_title(&desired);
            last_title = desired;
        }

        for px in canvas.iter_mut() {
            *px = BG;
        }
        draw_layout_bar(&mut canvas, canvas_w, layout);
        draw_mse_bar(&mut canvas, canvas_w, mse);
        let top = LAYOUT_BAR_H + MSE_BAR_H;
        let avail_h = canvas_h - top;
        let scale = pick_scale(layout, &left, &right, canvas_w, avail_h);
        match layout {
            Layout::SideBySide => {
                render_side_by_side(&mut canvas, canvas_w, canvas_h, top, &left, &right, scale)
            }
            Layout::OverUnder => {
                render_over_under(&mut canvas, canvas_w, canvas_h, top, &left, &right, scale)
            }
            Layout::Slider => {
                let max_w = left.width.max(right.width) * scale;
                let mouse_x = window
                    .get_mouse_pos(MouseMode::Clamp)
                    .map(|(x, _)| x as usize)
                    .unwrap_or(max_w / 2)
                    .min(max_w);
                render_slider(
                    &mut canvas,
                    canvas_w,
                    canvas_h,
                    top,
                    &left,
                    &right,
                    mouse_x,
                    scale,
                );
            }
            Layout::SliderV => {
                let max_h = left.height.max(right.height) * scale;
                let mouse_y = window
                    .get_mouse_pos(MouseMode::Clamp)
                    .map(|(_, y)| y as usize)
                    .unwrap_or(top + max_h / 2)
                    .saturating_sub(top)
                    .min(max_h);
                render_slider_v(
                    &mut canvas,
                    canvas_w,
                    canvas_h,
                    top,
                    &left,
                    &right,
                    mouse_y,
                    scale,
                );
            }
            Layout::Toggle => render_toggle(
                &mut canvas,
                canvas_w,
                canvas_h,
                top,
                &left,
                &right,
                showing_right,
                scale,
            ),
        }

        if let Err(e) = window.update_with_buffer(&canvas, canvas_w, canvas_h) {
            error!("frame_inspector update failed: {e}");
            break;
        }
    }
}

fn initial_title() -> &'static str {
    "frame_inspector — 1/2/3/4/5 layout  Esc close"
}

fn title_for(layout: Layout, showing_right: bool, left: &Image, right: &Image) -> String {
    match layout {
        Layout::Toggle => {
            let side = if showing_right {
                &right.label
            } else {
                &left.label
            };
            format!("frame_inspector — toggle: {side} (click to swap)")
        }
        _ => initial_title().to_string(),
    }
}

/// Render the layout list at the top of the canvas. The currently
/// selected layout is bracketed.
fn draw_layout_bar(canvas: &mut [u32], canvas_w: usize, current: Layout) {
    let mut s = String::new();
    for (i, (l, name)) in ALL_LAYOUTS.iter().enumerate() {
        if i > 0 {
            s.push_str("  ");
        }
        if *l == current {
            s.push('[');
            s.push_str(name);
            s.push(']');
        } else {
            s.push(' ');
            s.push_str(name);
            s.push(' ');
        }
    }
    draw_text_scaled(
        canvas,
        canvas_w,
        LAYOUT_BAR_H,
        BAR_PAD,
        BAR_PAD,
        &s,
        LABEL_COLOR,
        BAR_SCALE,
    );
}

/// Render the most recently reported MSE on its own line below the
/// layout bar.
fn draw_mse_bar(canvas: &mut [u32], canvas_w: usize, mse: Option<f64>) {
    let s = match mse {
        Some(v) => format!("mse={v:.3}"),
        None => "mse=n/a".to_string(),
    };
    draw_text_scaled(
        canvas,
        canvas_w,
        LAYOUT_BAR_H + MSE_BAR_H,
        BAR_PAD,
        LAYOUT_BAR_H + BAR_PAD,
        &s,
        LABEL_COLOR,
        BAR_SCALE,
    );
}

/// Pick a frame scale (1× or 2×) for the given layout. Returns 2 only
/// when 2× frames still fit inside the canvas alongside the layout's
/// fixed extras (gaps, labels). Frames are pixel-doubled — no
/// resampling, no aliasing logic.
fn pick_scale(
    layout: Layout,
    left: &Image,
    right: &Image,
    canvas_w: usize,
    avail_h: usize,
) -> usize {
    let max_w = left.width.max(right.width);
    let max_h = left.height.max(right.height);
    let (need_w, need_h) = match layout {
        Layout::SideBySide => (
            2 * left.width + GAP + 2 * right.width,
            2 * max_h + LABEL_PAD + GLYPH_H,
        ),
        Layout::OverUnder => (
            2 * max_w + LABEL_PAD + LABEL_W,
            2 * left.height + GAP + 2 * right.height,
        ),
        Layout::Slider => (2 * max_w, 2 * max_h + LABEL_PAD + GLYPH_H),
        Layout::SliderV => (2 * max_w + LABEL_PAD + LABEL_W, 2 * max_h),
        Layout::Toggle => (2 * max_w, 2 * max_h + LABEL_PAD + 2 * GLYPH_H),
    };
    if need_w <= canvas_w && need_h <= avail_h {
        2
    } else {
        1
    }
}

fn render_side_by_side(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    top: usize,
    left: &Image,
    right: &Image,
    scale: usize,
) {
    let lw = left.width * scale;
    let lh = left.height * scale;
    let rh = right.height * scale;
    blit_scaled(canvas, canvas_w, left, 0, top, scale);
    blit_scaled(canvas, canvas_w, right, lw + GAP, top, scale);
    let label_y = top + lh.max(rh) + LABEL_PAD;
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        0,
        label_y,
        &left.line(),
        LABEL_COLOR,
    );
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        lw + GAP,
        label_y,
        &right.line(),
        LABEL_COLOR,
    );
}

fn render_over_under(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    top: usize,
    left: &Image,
    right: &Image,
    scale: usize,
) {
    let lh = left.height * scale;
    let rh = right.height * scale;
    blit_scaled(canvas, canvas_w, left, 0, top, scale);
    blit_scaled(canvas, canvas_w, right, 0, top + lh + GAP, scale);
    let max_w = (left.width.max(right.width)) * scale;
    let label_x = max_w + LABEL_PAD;
    let left_label_y = top + lh.saturating_sub(GLYPH_H) / 2;
    let right_label_y = top + (lh + GAP) + rh.saturating_sub(GLYPH_H) / 2;
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        label_x,
        left_label_y,
        &left.line(),
        LABEL_COLOR,
    );
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        label_x,
        right_label_y,
        &right.line(),
        LABEL_COLOR,
    );
}

fn render_slider(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    top: usize,
    left: &Image,
    right: &Image,
    slider_x: usize,
    scale: usize,
) {
    let lw = left.width * scale;
    let rw = right.width * scale;
    let lh = left.height * scale;
    let rh = right.height * scale;
    let h = lh.max(rh);
    for y in 0..h {
        let dst_row = (top + y) * canvas_w;
        if y < lh {
            let cols = lw.min(slider_x);
            let src_row = (y / scale) * left.width;
            for x in 0..cols {
                canvas[dst_row + x] = left.pixels[src_row + x / scale];
            }
        }
        if y < rh && slider_x < rw {
            let src_row = (y / scale) * right.width;
            for x in slider_x..rw {
                canvas[dst_row + x] = right.pixels[src_row + x / scale];
            }
        }
    }
    let max_w = lw.max(rw);
    if slider_x < max_w {
        for y in 0..h {
            canvas[(top + y) * canvas_w + slider_x] = DIVIDER;
        }
    }
    // Labels go below their side of the curtain so it's obvious
    // which pixels came from which dump regardless of slider position:
    // left at x=0, right pinned to the right edge of the label area.
    // When the frame is narrower than the combined label widths we
    // expand the label area outward so the two never overlap and the
    // left/right placement still maps to the slider sides.
    let label_y = top + h + LABEL_PAD;
    let left_text = left.line();
    let right_text = right.line();
    let left_text_w = left_text.chars().count() * GLYPH_W;
    let right_text_w = right_text.chars().count() * GLYPH_W;
    let label_area_w = max_w.max(left_text_w + GLYPH_W + right_text_w);
    let right_x = label_area_w.saturating_sub(right_text_w);
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        0,
        label_y,
        &left_text,
        LABEL_COLOR,
    );
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        right_x,
        label_y,
        &right_text,
        LABEL_COLOR,
    );
}

fn render_slider_v(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    top: usize,
    left: &Image,
    right: &Image,
    slider_y: usize,
    scale: usize,
) {
    let lw = left.width * scale;
    let rw = right.width * scale;
    let lh = left.height * scale;
    let rh = right.height * scale;
    for y in 0..slider_y.min(lh) {
        let dst_row = (top + y) * canvas_w;
        let src_row = (y / scale) * left.width;
        for x in 0..lw {
            canvas[dst_row + x] = left.pixels[src_row + x / scale];
        }
    }
    for y in slider_y..rh {
        let dst_row = (top + y) * canvas_w;
        let src_row = (y / scale) * right.width;
        for x in 0..rw {
            canvas[dst_row + x] = right.pixels[src_row + x / scale];
        }
    }
    let max_w = lw.max(rw);
    let max_h = lh.max(rh);
    if slider_y < max_h {
        let dst_row = (top + slider_y) * canvas_w;
        for x in 0..max_w {
            canvas[dst_row + x] = DIVIDER;
        }
    }
    // Labels go on the right of the frame area, top label aligned with
    // the upper (left/expected) half, bottom label with the lower
    // (right/actual) half — so the side of the curtain each label
    // refers to is unambiguous.
    let label_x = max_w + LABEL_PAD;
    let upper_label_y = top + (max_h / 4).saturating_sub(GLYPH_H / 2);
    let lower_label_y = top + (3 * max_h / 4).saturating_sub(GLYPH_H / 2);
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        label_x,
        upper_label_y,
        &left.line(),
        LABEL_COLOR,
    );
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        label_x,
        lower_label_y,
        &right.line(),
        LABEL_COLOR,
    );
}

fn render_toggle(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    top: usize,
    left: &Image,
    right: &Image,
    showing_right: bool,
    scale: usize,
) {
    let visible = if showing_right { right } else { left };
    blit_scaled(canvas, canvas_w, visible, 0, top, scale);
    let label_y = top + (left.height.max(right.height)) * scale + LABEL_PAD;
    let mark = |on: bool| if on { "  <-- shown" } else { "" };
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        0,
        label_y,
        &format!("{}{}", left.line(), mark(!showing_right)),
        LABEL_COLOR,
    );
    draw_text(
        canvas,
        canvas_w,
        canvas_h,
        0,
        label_y + GLYPH_H,
        &format!("{}{}", right.line(), mark(showing_right)),
        LABEL_COLOR,
    );
}

fn blit_scaled(canvas: &mut [u32], canvas_w: usize, src: &Image, x: usize, y: usize, scale: usize) {
    if scale == 1 {
        for row in 0..src.height {
            let dst_off = (y + row) * canvas_w + x;
            let src_off = row * src.width;
            canvas[dst_off..dst_off + src.width]
                .copy_from_slice(&src.pixels[src_off..src_off + src.width]);
        }
        return;
    }
    let scaled_w = src.width * scale;
    let scaled_h = src.height * scale;
    for row in 0..scaled_h {
        let src_off = (row / scale) * src.width;
        let dst_off = (y + row) * canvas_w + x;
        for col in 0..scaled_w {
            canvas[dst_off + col] = src.pixels[src_off + col / scale];
        }
    }
}

fn drain_until_closed(rx: Receiver<Pair>) {
    while rx.recv().is_ok() {}
}

/// Pack RGBA8 into the 0x00RRGGBB u32s minifb expects.
fn rgba_to_minifb(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4)
        .map(|p| ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | (p[2] as u32))
        .collect()
}

/// Stamp `text` at the default [`FONT_SCALE`].
fn draw_text(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    x: usize,
    y: usize,
    text: &str,
    color: u32,
) {
    draw_text_scaled(canvas, canvas_w, canvas_h, x, y, text, color, FONT_SCALE);
}

/// Stamp `text` onto the canvas at `(x, y)` using the 8×8 bitmap
/// font scaled by `scale`. Glyphs that fall outside the canvas are
/// clipped silently.
fn draw_text_scaled(
    canvas: &mut [u32],
    canvas_w: usize,
    canvas_h: usize,
    x: usize,
    y: usize,
    text: &str,
    color: u32,
    scale: usize,
) {
    let glyph_w = 8 * scale + scale;
    let mut cx = x;
    for ch in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row, byte) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if byte & (1 << col) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = cx + col * scale + dx;
                            let py = y + row * scale + dy;
                            if px < canvas_w && py < canvas_h {
                                canvas[py * canvas_w + px] = color;
                            }
                        }
                    }
                }
            }
        }
        cx += glyph_w;
        if cx >= canvas_w {
            break;
        }
    }
}
