//! Group 10 — Side channel.
//!
//! Side channels require `wgpu_ctx` (for video) and a Unix socket directory
//! to be supplied via `PipelineCtx`. The current test harness intentionally
//! avoids `PipelineCtx` to stay decoupled from the full pipeline. These
//! tests are stubbed with `#[ignore]` and a note pointing to what would be
//! needed to enable them: a `VideoSideChannel`/`AudioSideChannel` test stub
//! that bypasses wgpu and the socket server. That stub belongs in the
//! `side_channel` module and is out of scope for this initial test pass.
//!
//! The cases (51–54) cover:
//!   - buffer caps without side channel (~100ms)
//!   - buffer growth up to `side_channel_delay` with side channel
//!   - PTS formula `frame.pts + offset - start_pts`
//!   - slow subscriber non-blocking

#[test]
#[ignore = "side channel tests require a wgpu/socket stub (out of scope for initial test pass)"]
fn buffer_caps_without_side_channel() {}

#[test]
#[ignore = "side channel tests require a wgpu/socket stub (out of scope for initial test pass)"]
fn buffer_grows_up_to_side_channel_delay() {}

#[test]
#[ignore = "side channel tests require a wgpu/socket stub (out of scope for initial test pass)"]
fn side_channel_pts_formula() {}

#[test]
#[ignore = "side channel tests require a wgpu/socket stub (out of scope for initial test pass)"]
fn slow_subscriber_does_not_block_queue() {}
