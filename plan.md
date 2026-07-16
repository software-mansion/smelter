# MoQ Client Output — Working Skeleton

## Context

Smelter recently gained MoQ client/server **inputs** (`smelter-core/src/pipeline/moq/`). This adds the publish half: a **MoQ client output** that connects to a relay, publishes a broadcast under a user-provided path, and streams encoded video/audio tracks described by a hang catalog. It reuses the already-present MoQ stack: `hang 0.19`, `moq-native 0.15`, `moq-mux 0.5.6`, `moq-msf 0.2`, `mp4-atom 0.11` — no new dependencies.

**Requirements (confirmed with user):**
- Connection identical to the MoQ client input: single `endpoint_url` (`https`/`http`), JWT embedded in the URL, no separate token field.
- `broadcast_path` is required.
- Video: h264, vp8, vp9. Audio: **Opus only** (confirmed; AAC may come later).
- Optional API field `container`: `legacy | cmaf | loc`, **default `cmaf`**.
- Both hang `catalog.json` and MSF `catalog` tracks published with the same info.
- **API shape: single `encoder` field** (RTP/RTMP style), not WHIP-style preferences (confirmed).
- **Reject BOTH vp8+cmaf and vp9+cmaf at registration** (confirmed) — CMAF is effectively H264-only for video; vp8/vp9 require explicitly choosing `legacy` or `loc`.
- Do NOT run `/api-change` and do NOT run tests at the end.

## Key verified facts

- Publish side of the client: `client.with_publish(origin.consume())` (`moq-native-0.15.0/src/client.rs:317`); keep the `OriginProducer`, after connect call `origin.publish_broadcast(path, broadcast.consume()) -> bool` (`moq-net-0.1.8/src/model/origin.rs:718`). Strip leading `/` from the path (the input compares with `trim_start_matches("/")`, `client_input.rs:119`).
- `moq_mux::catalog::Producer::new(&mut BroadcastProducer)` (`moq-mux-0.5.6/src/catalog/producer.rs:45`) creates **both** the hang `catalog.json` and MSF `catalog` tracks; `producer.lock()` guard publishes both identically on drop. Dual-catalog requirement satisfied for free.
- `moq_mux::container::Producer::<moq_mux::catalog::hang::Container>::new(track, container)` handles all 3 wire formats (`Legacy | Cmaf(fmp4::Wire) | Loc`); `write(Frame { timestamp, payload, keyframe })` manages MoQ groups (keyframe ⇒ new group). Writes are synchronous (network I/O happens in session tasks on `ctx.tokio_rt`).
- Do **not** use `moq_mux::import::Framed` — its codec importers hardcode `Container::Legacy`.
- CMAF: build the init segment (ftyp+moov) ourselves with `mp4_atom` (moq-mux's builders are `pub(crate)`; recipe in `moq-mux-0.5.6/src/container/fmp4/mod.rs:293-481`). `fmp4::Wire` only reads `mdhd.timescale`, `tkhd.track_id`, and `stsd` from the trak. Init bytes go into `hang::catalog::Container::Cmaf { init }` and `moq_mux::catalog::hang::Container::try_from(&hang_container)` builds the wire container from them.
- **H264 bitstream format is tied to the container** (user requirement):
  - **CMAF ⇒ AVCC**: `bitstream_format: H264BitstreamFormat::Avcc` (both FfmpegH264 and VulkanH264); encoder extradata is an avcC record — set it as the catalog `description` field AND parse it via `mp4_atom::Avcc::decode_body` for the init segment. Catalog codec: `hang::catalog::H264 { inline: false, profile: avcc[1], constraints: avcc[2], level: avcc[3] }`.
  - **Legacy and LOC ⇒ Annex B**: `bitstream_format: H264BitstreamFormat::AnnexB`; SPS/PPS inline in the bitstream, catalog codec `H264 { inline: true, .. }`, `description: None` (profile/constraints/level parsed from the SPS NAL in extradata when present: payload bytes 1–3 equal avcC bytes 1–3).
- FFmpeg encoder uses `bf=0`, so pts==dts.
- Templates: `RtmpClientOutput` (`smelter-core/src/pipeline/rtmp/rtmp_output.rs`) for encoder spawning/extradata/Output impl; `mp4_output.rs:296-368` for the single-channel writer loop + EOS state; `client_input.rs:62-89` for connect.

## Implementation

### 1. Core options — `smelter-core/src/protocols/moq.rs`

```rust
pub struct MoqClientOutputOptions {
    pub endpoint_url: Arc<str>,
    pub broadcast_path: Arc<str>,
    pub container: MoqOutputContainer,   // Legacy | Cmaf (default) | Loc
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}
```
Extend `MoqClientError`: `PublishFailed(...)`, `MissingH264DecoderConfig`, `InitSegmentError(...)`, `UnsupportedCodecContainer { codec, container }`.

### 2. Core enums / dispatch

- `smelter-core/src/output.rs`: `ProtocolOutputOptions::MoqClient(...)`, `OutputProtocolKind::MoqClient` (Display: `"moq_client"`).
- `smelter-core/src/error.rs`: `OutputInitError::MoqClientError(#[from] MoqClientError)`.
- `smelter-core/src/pipeline/output.rs` (`new_external_output`): arm constructing `MoqClientOutput::new(ctx, output_ref, opt)` → `(Box::new(output), None)`.
- Fix all exhaustive matches the compiler flags.

### 3. New module — `smelter-core/src/pipeline/moq/output/`

Register in `pipeline/moq/mod.rs` (`mod output; pub use output::MoqClientOutput;`).

**`output/mod.rs`** — `mod client_output; mod init_segment; mod track;`

**`output/client_output.rs`** — `MoqClientOutput { video: Option<VideoEncoderThreadHandle>, audio: Option<AudioEncoderThreadHandle> }`, `impl Output` like `rtmp_output.rs:277-296`, `kind() = MoqClient`.

`new()` flow:
1. `StatsEvent::NewOutput { kind: MoqClient }`.
2. Validate codec×container: **error `UnsupportedCodecContainer` for Vp8/Vp9 + Cmaf** (defense-in-depth; primary rejection in smelter-api).
3. One shared `crossbeam_channel::bounded(1000)` chunk channel; spawn `VideoEncoderThread`/`AudioEncoderThread` per the match blocks in `rtmp_output.rs:149-274` (all of FfmpegH264 | VulkanH264(guarded) | FfmpegVp8 | FfmpegVp9; audio: Opus only).
4. Build track setups (`output/track.rs`): hang `VideoConfig`/`AudioConfig` + wire container per track.
5. `connect()` — mirror `client_input.rs:62-89` but publish side:
   `Origin::random().produce()` → keep `OriginProducer`, `client.with_publish(origin.consume())`, `ctx.tokio_rt.block_on(client.connect(url))`, wrap in `MoqSession`.
6. `Broadcast::new().produce()` → `moq_mux::catalog::Producer::new(&mut broadcast)`; `broadcast.create_track(Track::new("video0" / "audio0"))` → `moq_mux::container::Producer::new(track, wire_container).with_lenient_start()`; insert renditions via one `catalog.lock()` guard (drop publishes hang + MSF catalogs); `origin.publish_broadcast(path_stripped, broadcast.consume())` — `false` ⇒ `PublishFailed`.
7. Spawn a std writer thread (`run_moq_output_thread`), moving in session/origin/broadcast/catalog/producers/receiver. On exit: `catalog.finish()`, drop session (`MoqSession::Drop` closes), emit `Event::OutputDone`.

Writer loop (EOS structure like `mp4_output.rs:296-368`):
- First-chunk PTS offset; `Timestamp::from_micros((pts - offset).as_micros() as u64)`.
- Video: `Frame { keyframe: chunk.is_keyframe, .. }` → `video_producer.write(...)`.
- Audio: `Frame { keyframe: true, .. }` → write + `finish_group()` (one group per frame — moq-mux importer convention, `codec/opus/import.rs:92-101`).
- `VideoEOS`/`AudioEOS` → `producer.finish()`; break when all present tracks are done.
- Write error ⇒ `Event::OutputError` (Critical, like mp4) + break. Send stats bytes-sent events per chunk.

**`output/init_segment.rs`** — CMAF init builders (H264 + Opus only, given the vp8/vp9+cmaf rejection):
- `Ftyp { isom, 0x200, [isom, iso6, mp41] }` + `Moov { mvhd, trak, mvex: Some(trex track_id 1) }` encoded via `mp4_atom::Encode`.
- Trak: `tkhd { track_id: 1, enabled, width/height }`, `mdhd { timescale }` (video 90_000; audio = sample_rate), `hdlr vide/soun`, `minf { vmhd|smhd, dinf/dref/url, stbl { stsd: [entry] } }`.
- Entries: H264 → `Avc1 { avcc: Avcc::decode_body(extradata) }`; Opus → `Opus { dops }` (recipe: `moq-mux fmp4/mod.rs:293-481`).

**`output/track.rs`** — encoder handle/options → (`hang::catalog::VideoConfig`/`AudioConfig`, `moq_mux::catalog::hang::Container`):
- H264 + CMAF: extradata (avcC) required else `MissingH264DecoderConfig`; codec `H264 { inline: false, profile/constraints/level from avcC }`; `description: Some(avcc)` in the catalog config (in addition to the avcC inside the init segment).
- H264 + Legacy/LOC: Annex B payload, codec `H264 { inline: true, .. }` (profile/constraints/level from the SPS in extradata when available), `description: None`.
- VP8: `VideoCodec::VP8`, no description. VP9: `VideoCodec::VP9(...)` with profile/chroma from `output_format` (yuv420→0, yuv422→1, yuv444→profile 1/3 pattern per `rtmp_output.rs:442-448`), conservative defaults elsewhere.
- Audio: `AudioConfig::new(AudioCodec::Opus, sample_rate, channels)`; no description.
- `coded_width/height` from encoder config; `framerate`/`bitrate` optional (fine to omit in skeleton).
- Container mapping: Legacy/Loc direct; Cmaf → build init (`init_segment.rs`) → `hang Container::Cmaf { init, timescale: Some(..), track_id: Some(1) }` (`#[allow(deprecated)]` for the two back-compat fields) → `try_from` for the wire container.

### 4. Stats — `smelter-core/src/stats/`

Copy `stats/output/rtmp.rs` → `stats/output/moq_client.rs` (events, state, sliding windows). Wire `MoqClient` variants into `stats/output/mod.rs` (`OutputStatsEvent`, `OutputStatsState::new`, `report()`, `handle_event()`, kind `From` impl) and `stats/output_reports.rs` (`OutputStatsReport::MoqClient` + report structs).

### 5. API layer — `smelter-api/src/output/`

**`moq_client.rs`** (derives + `deny_unknown_fields` like `input/moq_client.rs`):
```rust
pub struct MoqClientOutput {
    pub endpoint_url: Arc<str>,            // https:// relay URL, JWT embedded in URL
    pub broadcast_path: Arc<str>,          // REQUIRED publish path
    pub container: Option<MoqOutputContainer>,  // (**default=`cmaf`**)
    pub video: Option<OutputMoqClientVideoOptions>,
    pub audio: Option<OutputMoqClientAudioOptions>,
}
#[serde(rename_all = "snake_case")]
pub enum MoqOutputContainer { Legacy, Cmaf, Loc }
```
- Video options: copy `OutputRtpVideoOptions` shape (`resolution`, `send_eos_when`, `encoder`, `initial`); encoder enum = 4 variants copied from `RtpVideoEncoderOptions` (`ffmpeg_h264 | ffmpeg_vp8 | ffmpeg_vp9 | vulkan_h264`).
- Audio options: RTP audio shape (`encoder` enum with `opus` only, like `RtpAudioEncoderOptions`).
- Doc-comment the CMAF restriction on the `container` field.

**`moq_client_into.rs`** — `TryFrom<MoqClientOutput> for core::RegisterOutputOptions`, modeled on `rtp_into.rs`:
- At-least-one-of video/audio validation.
- **Registration-time rejection: `ffmpeg_vp8`/`ffmpeg_vp9` encoder + resolved container `Cmaf` ⇒ `TypeError`** telling the user to pick `legacy` or `loc`.
- `container.unwrap_or(Cmaf)`; H264 encoders get `bitstream_format` **derived from the resolved container**: `Avcc` for CMAF, `AnnexB` for Legacy/LOC.

Wire in `smelter-api/src/output.rs` (`mod` + `pub use`).

### 6. Routes — `src/routes/register_request.rs`

Import `MoqClientOutput`; add `RegisterOutput::MoqClient(MoqClientOutput)` (⇒ `"type": "moq_client"`); add `handle_output` arm calling `Pipeline::register_output(...)`.

## Execution order

1. Steps 1–2 (options, enums, errors).
2. `init_segment.rs` + `track.rs` (pure helpers).
3. `client_output.rs` + module wiring + dispatch arm.
4. Stats (compiler-driven).
5. API types + conversion.
6. Routes.
7. `cargo check --workspace` (fix flagged exhaustive matches).

**Explicitly out of scope (per user):** running `/api-change`, ts-sdk changes, running tests.

## Verification

- `cargo check` / `cargo clippy` on the workspace compiles clean.
- Manual smoke test (user-driven, not part of this task): register a `moq_client` output against a relay (e.g. the same relay used for MoQ input dev) and subscribe with the existing MoQ client input or `hang` web player; verify both `catalog.json` and `catalog` tracks appear and frames flow for each container × codec combo (h264+cmaf, vp8/vp9+legacy/loc, opus).
- Registration validation: vp8/vp9 with `container: cmaf` (or defaulted) returns a 400 with a clear message.

## Known limitations (accepted for skeleton)

- Relay disconnect after connect only surfaces via session death; no session-watch task yet (EOS path still terminates cleanly).
- VP9 catalog profile/level fields use conservative defaults derived from pixel format.
- Zero write-latency batching (`with_latency`) — one container frame per sample; future knob for CMAF packing.
