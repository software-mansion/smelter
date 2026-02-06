# Smelter Project

Real-time video compositor with Rust backend and TypeScript SDK.

## Commands

### Rust

```bash
# Build (without web-renderer for faster builds and to avoid building process_helper)
cargo build --no-default-features

# Build release
cargo build -r --no-default-features

# Run tests
cargo nextest run --workspace --profile ci

# Run specific test
cargo nextest run --workspace <test_name>

# Update snapshots (for new/changed tests)
cargo nextest run --workspace --features update_snapshots <test_name>

# Run binary
cargo run --bin main_process
cargo run -p <crate_name> --bin <binary_name>

# Run example
cargo run -p integration-tests --example simple

# Generate JSON schema (after API changes)
cargo run -p tools --bin generate_json_schema

# Lint
cargo clippy --workspace
```

### TypeScript (run from ./ts directory)

```bash
# Install dependencies
pnpm install

# Build all (including WASM)
pnpm build:all

# Build Node.js SDK only
pnpm build:node-sdk

# Build web-wasm SDK
pnpm build:web-wasm

# Build web-client SDK
pnpm build:web-client

# Build (JS only, no WASM rebuild)
pnpm build

# Typecheck
pnpm typecheck

# Lint
pnpm lint

# Generate types from JSON schema (after Rust API changes)
pnpm generate-types

# Run Node.js example (from ./ts/examples/node-examples)
pnpm run ts-node ./src/<example>.tsx
```

## Project Structure

### Rust Crates (Cargo workspace)

- `smelter` (root) - HTTP API server, main event loop
  - `main_process` - Starts the Smelter HTTP server
  - `process_helper` - Chromium subprocess helper (requires `web-renderer` feature)
- `smelter-api` - HTTP types, JSON serialization, type conversion
- `smelter-core` - Main library: queue logic, encoding/decoding, muxing, audio mixing
- `smelter-render` - GPU rendering, composition, scene layouts
- `smelter-render-wasm` - WASM bindings for smelter-render
- `vk-video` - Vulkan Video hardware decoding (used by smelter-core)
- `libcef` - Chromium Embedded Framework bindings (web rendering)
- `decklink` - DeckLink SDK bindings (hardware input)
- `rtmp` - RTMP protocol
- `integration-tests` - Snapshot tests (render + pipeline)
  - `benchmark` - GPU rendering performance benchmarks
  - `generate_rtp_from_file` - Converts media files to RTP stream dumps
  - `play_rtp_dump` - Plays back RTP dump files via GStreamer
  - `generate_frequencies` - Generates audio test files with specific frequencies
- `tools` - Internal utils, release scripts
  - `generate_json_schema` - Generates JSON schema from API types
  - `generate_docs_examples` - Renders example scenes for documentation
  - `generate_docs_example_inputs` - Generates input MP4 files for doc examples
  - `generate_playground_inputs` - Generates input files for the playground
  - `package_for_release` - Bundles Smelter for platform-specific release
  - `dependency_check` - Validates runtime dependencies (e.g. FFmpeg version)

### TypeScript Packages (pnpm workspace in ./ts)

- `@swmansion/smelter` - React components and API types
- `@swmansion/smelter-core` - Base implementation used internally by all the runtime specific SDKs.
- `@swmansion/smelter-node` - Node.js SDK
- `@swmansion/smelter-web-wasm` - Browser SDK with WASM
- `@swmansion/smelter-browser-render` - WASM rendering engine
- `@swmansion/smelter-web-client` - Web client
- `create-smelter-app` - CLI scaffolding tool (`npx create-smelter-app`), generates projects from templates

### create-smelter-app (./ts/create-smelter-app)

CLI tool for scaffolding new Smelter projects. Supports npm/yarn/pnpm.

Available templates (Node.js):
- `node-minimal` - Streams a simple static layout to a local RTMP server
- `node-express-zustand` - Express.js + Zustand with HTTP API for dynamic layout control
- `node-offline-minimal` - Generates an MP4 file with a single static layout
- `node-offline-showcase` - Generates an MP4 by combining multiple source MP4 files

### smelter-core Architecture (./smelter-core)

- `pipeline` - Central `Pipeline` struct managing inputs, outputs, queue, renderer, and audio mixer. Spawns renderer and audio mixer threads. Protocol-specific submodules: `rtp`, `rtmp`, `mp4`, `hls`, `webrtc`, `channel`, `v4l2`, `decklink`
- `queue` - Synchronizes frames/samples from multiple inputs into batched output sets at a fixed framerate. Separate `VideoQueue` and `AudioQueue` driven by a `QueueThread`
- `audio_mixer` - Mixes audio samples from multiple inputs per output, supports volume control and mono/stereo mixing strategies
- `codecs` - Encoder/decoder options and types for H.264, VP8, VP9, Opus, AAC (FFmpeg and Vulkan backends)
- `protocols` - Protocol-specific input/output option types (RTP, RTMP, MP4, HLS, WebRTC WHIP/WHEP, V4L2, DeckLink)
- `graphics_context` - wgpu device/queue initialization and management

#### Pipeline Data Flow

1. **Input** - Protocol-specific receivers (RTP/RTMP/MP4/HLS/WebRTC/DeckLink/V4L2) receive encoded data and spawn decoder threads that produce raw `Frame`s (video) and `InputAudioSamples` (audio). Decoded data is sent to the queue via channels.
2. **Queue** - `QueueThread` runs a tick loop driven by the output framerate. On each tick it collects the latest frame from every input's `VideoQueue` and a chunk of samples from every input's `AudioQueue`, producing `QueueVideoOutput` and `QueueAudioOutput` batched sets. Frames/samples that arrive too late are dropped (unless `never_drop_output_frames` is set). The queue also dispatches scheduled events at the correct PTS.
3. **Renderer thread** - Receives `QueueVideoOutput` from the queue, passes the `FrameSet` to `Renderer::render()` (from `smelter-render`) which composites all inputs according to each output's scene graph, and sends the resulting output frames to per-output channels.
4. **Audio mixer thread** - Receives `QueueAudioOutput` from the queue, resamples and mixes input samples per output according to `AudioMixerConfig` (volume, mixing strategy), and sends `OutputAudioSamples` to per-output channels.
5. **Output** - Per-output encoder threads consume rendered frames / mixed audio, encode them (H.264/VP8/VP9, Opus/AAC), and send encoded chunks to protocol-specific senders (RTP/RTMP/MP4/HLS/WebRTC).

`Pipeline::start()` wires it all together: it creates bounded channels between queue → renderer thread and queue → audio mixer thread, then spawns both threads.

## API Changes Process

1. Update Rust code
2. Run `cargo run -p tools --bin generate_json_schema`
3. Run `pnpm generate-types` in ./ts
4. Update TypeScript code
5. Update CHANGELOG

## Testing Notes

- Snapshot tests for rendering: `./integration-tests/src/render_tests`
- Pipeline tests: `./integration-tests/src/pipeline_tests`
- Snapshots stored in git submodule: run `git submodule update --init --checkout` initially
- Pipeline tests can be flaky under CPU load (nextest profile limits concurrency)

## Code Style

- Rust: Follow clippy lints (todo, uninlined_format_args warnings)
- Rust: Keep use/mod statements in this order with empty lines between blocks (std, external crate, current crate, prelude::* if needed/available, pub mod, pub use)
- TypeScript: ESLint + Prettier configured
