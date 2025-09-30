# Development process

- [Rust](#rust)
  - [Crates](#crates)
    - [`smelter` (root crate)](#smelter-root-crate)
    - [`smelter_api`](#smelter_api)
    - [`smelter_core`](#smelter_core)
    - [`smelter_render`](#smelter_render)
    - [`smelter_render_wasm`](#smelter_render_wasm)
    - [`tools`](#tools)
    - [`integration_tests`](#integration_tests)
  - [Binaries](#binaries)
  - [Examples](#examples)
  - [Tests](#tests)
- [TypeScript SDK](#typescript-sdk)
  - [Packages](#packages)
  - [Examples](#examples)

# Rust

## Crates

### `smelter` (root crate)

Implements smelter server.

Actual create implements:
- HTTP API
- parsing configuration options
- configures logging
- runs main event loop.

However, most calls are just proxied to `Pipeline` instance from `smelter_core`. JSON parsing
is handled by `smelter_api` crate.

### `smelter_api`

Crate includes:
- HTTP type definition (with JSON serialization and deserialization logic)
- Type conversion from `smelter_api` types to `smelter_core` and `smelter_render`.
  In some cases this conversion also applies default values.

It is used by:
- `smelter` (root crate) to parse HTTP request
- `smelter_render_wasm` to parse JSON object in WASM implementation

### `smelter_core`

Smelter as a library, main entrypoint if you want to use smelter from Rust directly.

Crate implements:
- Queue logic
- Encoding and decoding
- Muxing/demuxing and transport protocols
- Audio mixing

It is using `smelter_render` to:
- Compose set of frames produced by queue (one frame per input) into set of composed frames (one frame per output)
- All scene updates for video are proxied to this crate

### `smelter_render`

Implements rendering for smelter.

Renderer receive set of frames (one per input) and is producing set of composed frames (one per output).
It is responsible for:
- If input frame is provided as bytes, then it creates GPU texture from them.
- if output frame needs to returned as bytes, then download it from GPU texture.
- Converting YUV/NV12 to RGBA on input, and reverse on output.
- Actual composition/layouts based on scene updates.
- State responsible for animations/transitions.

Users of this crate are responsible for handling decoding/encoding/protocols/queuing and triggering render.

The 2 core entrypoints of this library are:
- `Renderer::render` method that takes set of input frames and produces set of output frames.
- `Renderer::update_scene` that changes layout that `render` method will use for inputs.

### `smelter_render_wasm`

Wraps `smelter_render` crate with WASM api.

It is used in TypeScript SDK:
- `./ts/smelter-browser-render` - to build WASM binary and expose rendering API as JS library.
- (in directly) `./ts/smelter-web-wasm` - to run Smelter in the browser

### `tools`

Internal utils, release scripts

### `integration_tests`

Integration tests:
- Rendering snapshot tests that render single frame and compare with snapshot version.
- Pipeline snapshot tests that generate entire video/audio and compare with snapshot version.

## Binaries

Directory **`src/bin`** in a Rust crate has a special meaning. Each **`*.rs`** file or directory
(with **`main.rs`**) is a binary that can be executed with:

```bash
# from current crate
cargo run --bin <binary_name>
# from a different crate in workspace
cargo run -p <crate_name> --bin <binary_name>
```

For example:

```
cargo run --bin main_process
```

to run **`./src/bin/main_process.rs`**

or

```
cargo run -p tools --bin package_for_relase
```

to run **`./tools/src/bin/package_for_release/main.rs`**.

### `smelter` (root crate)

- `main_process` - main binary to start standalone compositor
- `process_helper` - starts secondary processes that communicate with `main_process`
  to enable web rendering with Chromium

### `integration_tests` crate

- `play_rtp_dump` - helper to play RTP dumps generated in tests from **`./integration_tests/src/pipeline_tests`**.
  Useful to verify if new tests generated correctly or to see what is wrong when test is failing.
- `generate_rtp_from_file` - helper to generate new input files that can be used for new tests in
  **`./integration_tests/src/tests`**.

### `tools` create

This is crate when we keep most of our internal tools and build scripts.

- `package_for_relase` - builds release binaries that can be uploaded to GitHub Releases
- `generate_docs_examples` - generates WEBP files with examples for documentation.
- `generate_docs_example_inputs` - helper for `generate_docs_examples` (generates inputs for that binary).
- `generate_json_schema` - generate JSON schema from `smelter_api` types. It needs to be called every
  time API is changed and regenerated file needs to be committed. (It also generates Markdown docs, but
  this will be soon removed)

## Examples

Similar to bins, examples have a specific directory in the Rust crate structure. Examples are placed
in the **`examples`** directory and can be run with:

```bash
# from current crate
cargo run --example <example_name>
# from a different crate in workspace
cargo run -p <crate_name> --example <example_name>
```

For example,

```
cargo run -p integration_tests --example simple
```

will run **`./integration_tests/examples/simple.rs`**.

## Tests

We have 3 main types of tests:
- A small number of regular unit tests spread over the codebase.
- Snapshot tests that render a specific image and save it as PNG.
  - Generated images are located in **`./integration_tests/snapshots/render_snapshots`** directory.
  - Tests and JSON files representing tested scenes are in **`./integration_tests/src/render_tests`** directory
  - For example:
    - Test is located in **`./integration_tests/src/render_tests/view_tests.rs`**
    - Test is rendering scene described by **`./integration_tests/src/render_tests/view/border_radius.scene.json`**
    - Test is rendering output (or comparing with the old version) to
      **`./integration_tests/snapshots/render_snapshots/view/border_radius_0_output_1.png`**
- Pipeline tests that are basically snapshot tests, but compare entire videos.
  - Generated stream dumps are located in **`./integration_tests/snapshots/rtp_packet_dumps`** directory.
  - Tests are implemented in **`./integration_tests/src/pipeline_tests`** directory.
  - Some of the tests here might be fragile if running along with a lot of CPU-intensive
    tasks. That is why **`./.config/nextest.toml`** is limiting the concurrency of some tests on CI.

Initially, when setting up the repo or when checking out a different branch, you need to update
submodule to the correct version by running
```
git submodule update --init --checkout
```

> Carefully read the output of the command to make sure it worked. It will fail if you have uncommitted
changes in the **`./snapshot_tests/snapshots`** directory.

To run all test run:

```
cargo nextest run --workspace --profile ci
```

If you don't have `nextest` installed you can add it with;

```
cargo install cargo-nextest
```

To run a specific test, add the name of the function or crate at the end:

e.g.

```
cargo nextest run --workspace audio_mixing_with_offset
```

will run a test from **`./integration_tests/src/pipeline_tests/audio_only.rs`**

#### Updating snapshots

By default, snapshot tests generate new output and compare it with the old version committed
to repo. If version in repo is different or missing, then tests will fail.

To generate snapshots for new tests, or update those that should change, you need to enable
`update_snapshots` feature when running tests. It is recommended to only run that command
for specific tests, especially **`./integration_tests/src/pipeline_tests`**

e.g.

```
cargo nextest run --workspace --features update_snapshots audio_mixing_with_offset
```

will run a test from **./integration_tests/src/pipeline_tests/audio_only.rs** and if output changed the update the snapshot.

If you made changes that modify the snapshot:
- Create PR in https://github.com/membraneframework-labs/video_compositor_snapshot_tests repo.
- Create PR in live compositor repo with link to the snapshot repo PR.
- Merge them together.

# TypeScript SDK

## Packages

- `@swmansion/smelter` - React components and common API types. (analog of `react` package).
- `@swmansion/smelter-core` - Base implementation that is used by packages like `@swmansion/smelter-node`.
- `@swmansion/smelter-node` - Node.js SDK for compositor  (analog of `react-dom` for Node.js)
- `@swmansion/smelter-web-wasm` - Browser SDK for compositor that includes compositor compiled to WASM (analog of `react-dom` for Node.js)
- `@swmansion/smelter-browser-render` - Rendering engine from Smelter compiled to WASM.
  - Run `pnpm run build-wasm` to build WASM bundle from Rust code
  - Run `pnpm run build` to build JS code (WASM has to be build earlier, or use `build:all` in root directory to build everything)

## API compatibility

SDK should always be compatible with the version of code from the repo. If you are changing the API, the changes to the SDK should land in the same PR.

## Examples

To bootstrap repo run:

```
pnpm install && pnpm build:all
```

After that, you can run only `pnpm run build` to rebuild if the Rust code did not change.

### Node.js: **`./ts/examples/node-examples/`**

To run example **`./ts/examples/node-examples/src/simple.tsx`**, go to **`./ts/examples/node-examples`** and run:

```
pnpm run ts-node ./src/simple.tsx
```

> TODO: instructions below needs to be automated to build Rust automatically and point examples to this binary

To run Node.js example against Rust code from the repo you need to in the root directory run:

```
cargo build -r --no-default-features
export SMELTER_PATH=$(pwd)/target/release/main_process
```

### Web WASM: **`./ts/examples/vite-browser-render`**

Run

```
pnpm run dev
```

Open `localhost:5173` in the browser

If Rust code changes you need to rebuild WASM with `pnpm run build-wasm`.

## Process for introducing API changes

Everything in the same PR.

- Update Rust code.
- Run `cargo run -p tools --bin generate_json_schema` that will generate **`./tools/schemas/scene.schema.json`** and **`./tools/schemas/api_types.schema.json`**.
- Run `pnpm run generate-types` in **`./ts`** that will generate **`./ts/smelter/src/api.generated.ts`**.
- Update TypeScript code to support new changes.
- Update CHANGELOG

> To avoid problems with forgetting about adding some changes to TS, everything that shows up in PR diff for
  `./ts/smelter/src/api.generated.ts` should be addressed in the PR that regenerated those types.
