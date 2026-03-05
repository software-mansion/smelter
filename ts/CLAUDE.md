# CLAUDE.md

# Overview

TypeScript SDK for Smelter server. Supports Node.js and Browser runtimes.

## Project-Specific Commands

Should be executed from `./ts` directory

```bash
# Build everything, it takes a long time because of WASM, in most cases
# you don't need to rebuild it, so run `pnpm run build` instead.
pnpm run build:all
```

## Architecture

**TypeScript SDK** (pnpm workspace in `./ts`, published under `@swmansion`):
- **`smelter`** — Main React component library for building video compositions. Public API entry point.
- **`smelter-core`** — Core React Fiber reconciler bridging React to the Smelter engine. It's used by other runtime specific packages like `smelter-core`.
- **`smelter-node`** — Node.js runtime. Manages WebSocket connections, file ops, HTTP to a Smelter server.
- **`smelter-browser-render`** — WASM rendering engine (compiled from Rust via wasm-pack).
- **`smelter-web-wasm`** — Full Smelter server running in-browser using WASM. Combines browser-render + core.
- **`smelter-web-client`** — Browser client for connecting to a remote Smelter server instance.
- **`create-smelter-app`** — CLI scaffolding tool for new Smelter projects from templates.

### Data Flow

- React component build using `@swmansion/smelter` package
- Pass component to Smelter instance initialized from runtime specific package e.g. `@swmansion/smelter-node`
- React will run the component, each change in virtual DOM is converted to Smelter scene update request and sent to the server:
  - In case of WASM smelter as a function call
  - In case of other packages as an HTTP request
