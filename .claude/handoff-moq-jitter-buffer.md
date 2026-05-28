# Handoff: MoQ Jitter Buffer — Extending to Dynamic

## Next session focus

Extend the currently implemented fixed-size MoQ jitter buffer to be **dynamic** — adapting buffer size based on observed network jitter rather than using a hardcoded 500ms fill threshold.

## Current state

- **Branch**: `@jbrs/moq-jitter-buffer`
- **Uncommitted changes** in `smelter-core/src/pipeline/moq/connection.rs` (156 insertions, 54 deletions)
- **Compiles**: `cargo check -p smelter-core` passes
- **Tests**: All pass except 5 pre-existing flaky timing tests in `rtcp_sync` (unrelated)
- **Not yet manually tested** with a live MoQ source

## What was implemented

A `MoqJitterBuffer` struct in `smelter-core/src/pipeline/moq/connection.rs` (line ~348) that:

1. **Fill phase**: Reads frames from `ContainerConsumer` until PTS span >= `MOQ_JITTER_BUFFER_SIZE` (500ms)
2. **Release phase**: `tokio::select!` loop with two branches:
   - Read new frames from consumer into `VecDeque<BufferedFrame>`
   - Sleep until next frame's wall-clock release time, then pop and send to decoder
3. **PTS calculation**: `anchor_pts + normalized_pts + buffer_size` where `anchor_pts = sync_point.elapsed()` at fill completion
4. **Drain behavior**: When buffer empties, timer becomes `pending()` — next arriving frame releases immediately (no re-fill, maintains timeline continuity)

### Key changes from baseline

| Area | Before | After |
|------|--------|-------|
| Constants | `MOQ_QUEUE_BUFFER = 2s` | `MOQ_JITTER_BUFFER_SIZE = 500ms` (new) |
| Queue offset | `Pts(effective_last_pts() + MOQ_QUEUE_BUFFER)` | `Pts(Duration::ZERO)` |
| Frame delivery | Inline loops in `read_video_track`/`read_audio_track` forwarding immediately | `MoqJitterBuffer::run()` with fill + paced release |
| PTS normalization | Done in read functions | Moved into jitter buffer |

### Struct fields

```rust
struct MoqJitterBuffer {
    buffer: VecDeque<BufferedFrame>,
    buffer_size: Duration,        // fixed 500ms — this is what to make dynamic
    sync_point: Instant,
    first_pts: Option<Duration>,
    wall_anchor: Option<Instant>, // when releasing begins
    anchor_pts: Option<Duration>, // sync_point.elapsed() at wall_anchor time
}
```

## Architecture context

- **File**: `smelter-core/src/pipeline/moq/connection.rs` — all jitter buffer code lives here
- **Data flow**: MoQ broadcast → `ContainerConsumer` → **jitter buffer** → `DecoderThreadHandle.chunk_sender` → decoder → queue
- **Both video and audio** use the same `MoqJitterBuffer` struct, instantiated separately in `run_video_track` / `run_audio_track`
- **RTP comparison**: RTP inputs have their own `RtpJitterBuffer` in `smelter-core/src/pipeline/rtp/` — different design (packet-level, handles reordering), but useful as a reference for dynamic adaptation patterns
- **Project CLAUDE.md**: `server/.claude/CLAUDE.md` and `server/smelter-core/CLAUDE.md` have architecture overview

## Considerations for dynamic extension

- `buffer_size` is currently a constant passed at construction — making it dynamic means updating it during the release phase based on observed inter-group arrival times
- The `output_pts` and `release_time` methods both depend on `buffer_size` — changing it mid-stream needs care to avoid PTS discontinuities
- `wall_anchor` / `anchor_pts` anchor the timeline; a dynamic buffer size change should probably adjust release pacing without resetting the anchor
- Consider whether `MOQ_JITTER_BUFFER_SIZE` becomes a minimum/maximum bound rather than a fixed value
- The fill phase threshold could also adapt (e.g., start with a conservative fill, then shrink)

## Suggested skills

- `/code-review` — review the uncommitted diff before extending further
- `/verify` — manually test with a MoQ source to confirm the fixed buffer works before adding dynamic behavior
