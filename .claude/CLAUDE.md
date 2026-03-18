# CLAUDE.md

## Project Overview

Smelter is a toolkit for real-time, low-latency, programmable video and audio composition. It combines multimedia from different sources into a single video or live stream, with support for text, custom shaders, and embedded websites.

## Architecture

**Core pipeline:**
- **`smelter` (root)** — HTTP server (Axum). Parses config, proxies calls to `smelter-core` Pipeline.
- **`smelter-api`** — HTTP request/response types with JSON serde. Converts API types to `smelter-core`/`smelter-render` types. Also used by `smelter-render-wasm`.
- **`smelter-core`** — Main library. Pipeline management, queue logic, encoding/decoding, muxing/transport protocols, audio mixing. Uses `smelter-render` for composition.
- **`smelter-render`** — GPU rendering engine (wgpu). Takes input frames → produces composed output frames. Handles YUV/NV12↔RGBA conversion, scene layout, animations/transitions. Two core entrypoints: `Renderer::render` and `Renderer::update_scene`.
- **`smelter-render-wasm`** — WASM wrapper around `smelter-render` for browser use.

**Libraries:**
- **`vk-video`** — Vulkan Video hardware codec (H.264 decode/encode), Linux/Windows only.
- **`libcef`** — Chromium Embedded Framework bindings (web rendering in compositions).
- **`decklink`** — DeckLink SDK bindings for Blackmagic capture cards.
- **`rtmp`** — RTMP protocol implementation.

**Utilities:**
- **`integration-tests`** — Snapshot tests for rendering and full pipeline. Create includes examples used for manual testing.
- **`tools`** — Internal utilities: `generate_from_types`, `package_for_release`, doc generation.

**Feature flags:** `web-renderer` (default, enables Chromium), `decklink`, `update_snapshots`.

**TypeScript SDK** - See `ts/CLAUDE.md` for details

#### Server control flow

HTTP API `smelter` crate → `smelter-api` (parse) → `smelter-core` (pipeline: inputs, queue, encoders, outputs) → `smelter-render` (GPU composition) → encoded output to transport.

## API Changes

After modifying types in `smelter-api` or types in `smelter-core::stats`, use `/api-change` to run the full generation and validation workflow.
