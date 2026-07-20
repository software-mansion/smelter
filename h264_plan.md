# Defer MoQ catalog publication for H.264 annexB until first keyframe

**IMPORTANT**: Implement the code using the Opus 4.8 subagent, DO NOT WRITE THE CODE YOURSELF.

## Context

For H.264 with the Legacy/Loc MoQ containers (annexB bitstream), the hang catalog advertises hardcoded profile/constraints/level bytes (`DEFAULT_H264_PROFILE = (0x42, 0xe0, 0x1e)`, constrained baseline 3.0) because the catalog is published in `MoqClientOutput::new` before any frame exists, and FFmpeg leaves `extradata` empty in annexB mode. Streams encoded at e.g. High 4.2 are therefore mis-advertised and may be falsely accepted then fail to decode.

Fix: for this one case, defer publication of the catalog contents until the first encoded video keyframe reaches the writer thread, parse profile/constraints/level from its inline SPS, publish the catalog once with the real values, and never touch it again (no catalog updates).

## Decisions (confirmed with user)

- **Scope:** defer only for H.264 + `Legacy`/`Loc` container. CMAF, VP8, VP9, and audio-only outputs keep the exact eager publish path they have today.
- **Trigger:** first video chunk with `is_keyframe == true` (SPS only lives in keyframes; `with_lenient_start()` already drops earlier deltas).
- **Parse failure:** publish immediately with `DEFAULT_H264_PROFILE` and log at **debug** level. Written once, never updated.
- **VideoEOS while still pending:** publish the catalog with default values so a concurrent audio rendition stays discoverable.
- **Both H.264 encoders** (FfmpegH264, VulkanH264) go through the same deferred path — no Vulkan special case even though its SPS is knowable at init.
- Broadcast announce (`origin.publish_broadcast`) and `ContainerProducer` track creation stay eager; only the catalog `insert` calls are deferred.

## Changes

### 1. New SPS helper — `smelter-core/src/pipeline/utils/h264_annexb_to_avcc.rs`

Add alongside the existing code, reusing `split_annexb_nalus` (line 10) and the same byte-extraction already used by `build_avc_decoder_config` (lines 97-99):

```rust
/// Reads (profile_idc, constraint_flags, level_idc) from the first SPS NAL in
/// an annexB stream. They are bytes 1-3 of the SPS payload.
pub(crate) fn annexb_h264_profile(data: &[u8]) -> Option<(u8, u8, u8)>
```

Find the first NAL with `nalu[0] & 0x1F == NALU_TYPE_SPS` and `len >= 4`, return `(nalu[1], nalu[2], nalu[3])`. Export from `utils/mod.rs`. Add a small unit test with a synthetic annexB buffer (SPS + PPS + IDR NALs) plus a no-SPS negative case.

### 2. `smelter-core/src/pipeline/moq/output/client_output.rs` — the main change

**`BroadcastState`** gains a field:

```rust
enum CatalogState {
    Published,
    /// H264 annexB: configs held back until the first keyframe's SPS is parsed.
    Pending {
        video: hang::catalog::VideoConfig,
        audio: Option<hang::catalog::AudioConfig>,
    },
}
```

**`MoqClientOutput::new` (~line 95):** compute whether to defer — video options match `FfmpegH264 | VulkanH264` **and** `options.container != MoqOutputContainer::Cmaf` (the exact condition of the hardcoded arm in `track.rs:63`). Pass this flag into `publish`.

**`publish` (lines 137-200):** everything stays as-is except the catalog-insert block (lines 167-181). When deferring, skip both `insert` calls and set `catalog_state: CatalogState::Pending { video, audio }` with the configs produced by `track::video`/`track::audio` (video config still carries the default profile bytes as placeholder). Otherwise insert as today and set `Published`.

**Writer thread `run_moq_output_thread` (lines 341-388):**
- On `EncodedOutputEvent::Data` with `MediaKind::Video(_)` and `chunk.is_keyframe`, if state is `Pending`: 
  1. `annexb_h264_profile(&chunk.data)` — on `Some`, patch the three fields on the pending config's codec (`match &mut config.codec { VideoCodec::H264(h264) => ... }`, `inline` stays `true`); on `None`, keep defaults and `debug!(...)`.
  2. Insert video (and audio if present) into the catalog under **one** `catalog.lock()` guard, mirroring the eager block. Insert errors → emit `Event::OutputError` with `ErrorSeverity::Critical` and break, same handling as `write_chunk` failures (lines 357-364).
  3. Set `Published`, then fall through to the normal `write_chunk` call (catalog lands before the first frame is written).
- On `EncodedOutputEvent::VideoEOS` while `Pending`: publish with the current (default) values before `finish(...)`.

### 3. `smelter-core/src/pipeline/moq/output/track.rs` — comment only

No structural change; `track::video` keeps producing the annexB config with `DEFAULT_H264_PROFILE`. Update the comment on `DEFAULT_H264_PROFILE` (lines 13-16) to say it is now a placeholder/fallback that is normally overwritten from the first keyframe's SPS before the catalog is published.

## Verification

1. `cargo check -p smelter-core`.
2. `cargo test -p smelter-core annexb` — the new helper's unit tests.
3. Manual end-to-end: run the demo (`integration-tests/examples/demo`, has a `moq_client` output) with an H.264 + Legacy/Loc MoQ output against a relay; subscribe (hang web player or the demo's moq input) and confirm the catalog's `avc1.PPCCLL` codec string now reflects the encoder's real profile/level (e.g. `avc1.64xxxx` for High) instead of `avc1.42e01e`, and that the catalog appears together with the first keyframe. Add a `debug!` log of the parsed `(profile, constraints, level)` to make this easy to confirm from smelter logs alone.
