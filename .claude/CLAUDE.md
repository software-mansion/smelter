# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Smelter is a toolkit for real-time, low-latency, programmable video and audio composition. It combines multimedia from different sources into a single video or live stream, with support for text, custom shaders, and embedded websites. Built by Software Mansion.

## Build & Development Commands

### Rust

```bash
# Build (default includes web-renderer feature)
cargo build
cargo build --release --no-default-features

# Run all tests (requires cargo-nextest: cargo install cargo-nextest)
cargo nextest run --workspace --profile ci

# Run a specific test by name
cargo nextest run --workspace audio_mixing_with_offset

# Update snapshot for a specific test
cargo nextest run --workspace --features update_snapshots TEST_NAME

# Run doctests (nextest doesn't support these)
cargo test --workspace --doc

# Lint
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D clippy::todo -D clippy::uninlined_format_args -D warnings

# Generate JSON schema and OpenAPI from API types (run after any API change)
cargo run -p tools --bin generate_from_types
```

### TypeScript SDK (in `./ts`)

```bash
pnpm install && pnpm build:all
pnpm run generate-types    # after running generate_from_types
```

## Architecture

Rust edition 2024, toolchain latest stable. Workspace with 10 crates:

**Core pipeline:**
- **`smelter` (root)** — HTTP server (Axum). Parses config, proxies calls to `smelter-core` Pipeline.
- **`smelter-api`** — HTTP request/response types with JSON serde. Converts API types to `smelter-core`/`smelter-render` types. Also used by `smelter-render-wasm`.
- **`smelter-core`** — Main library. Pipeline management, queue logic, encoding/decoding, muxing/transport protocols, audio mixing. Uses `smelter-render` for composition.
- **`smelter-render`** — GPU rendering engine (wgpu). Takes input frames → produces composed output frames. Handles YUV/NV12↔RGBA conversion, scene layout, animations/transitions. Two core entrypoints: `Renderer::render` and `Renderer::update_scene`.
- **`smelter-render-wasm`** — WASM wrapper around `smelter-render` for browser use.

**Libraries:**
- **`vk-video`** — Vulkan Video hardware codec (H.264 decode/encode), Linux/Windows only.
- **`libcef`** — Chromium Embedded Framework bindings (web rendering in compositions).
- **`decklink`** — DeckLink SDK bindings for professional capture hardware.
- **`rtmp`** — RTMP protocol implementation.

**Utilities:**
- **`integration-tests`** — Snapshot tests for rendering and full pipeline.
- **`tools`** — Internal utilities: `generate_from_types`, `package_for_release`, doc generation.

**Feature flags:** `web-renderer` (default, enables Chromium), `decklink`, `update_snapshots`.

**TypeScript SDK** (pnpm workspace in `./ts`, published under `@swmansion`):
- **`smelter`** — Main React component library for building video compositions. Public API entry point.
- **`smelter-core`** — Core React Fiber reconciler bridging React to the Smelter engine. Foundation for runtime-specific packages.
- **`smelter-node`** — Node.js runtime. Manages WebSocket connections, file ops, HTTP to a Smelter server.
- **`smelter-browser-render`** — WASM rendering engine (compiled from Rust via wasm-pack). GPU rendering in browser.
- **`smelter-web-wasm`** — Full Smelter server running in-browser using WASM. Combines browser-render + core.
- **`smelter-web-client`** — Browser client for connecting to a remote Smelter server instance.
- **`create-smelter-app`** — CLI scaffolding tool for new Smelter projects from templates.

### Data Flow

HTTP API → `smelter-api` (parse) → `smelter-core` (pipeline: inputs, queue, encoders, outputs) → `smelter-render` (GPU composition) → encoded output to transport.

Input protocols: RTP, RTMP, MP4, WebRTC, DeckLink, V4L2. Output protocols: RTP, RTMP, HLS, MP4, WebRTC.

## API Changes

After modifying types in `smelter-api`, use `/api-change` to run the full generation and validation workflow.

## Key Binaries

- `cargo run --bin main_process` — main compositor server
- `cargo run --bin process_helper` — secondary process for web rendering with Chromium
