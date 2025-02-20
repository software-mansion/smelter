# Development process

## Rust

### Binaries

Directory **`src/bin`** in Rust crate have a special meaning. Each **`*.rs`** file or directory
(with **`main.rs`**) is a binary that can be executed with:

```
cargo run --bin <binary_name>
```

For example:

```
cargo run --bin package_for_relase
```
to run **`./src/bin/package_for_release/main.rs`**.

or

```
cargo run --bin main_process
```

to run **`./src/bin/main_process.rs`**


To run binary from specific create you need to run the command from that crate directory.

#### `smelter` create (root)

- `package_for_relase` - builds release binaries that can be uploaded to GitHub Releases
- `main_process` - main binary to start standalone compositor
- `process_helper` - starts secondary processes that communicate with `main_process`
  to enable web rendering with Chromium

#### `integration_tests` crate

- `play_rtp_dump` - helper to play RTP dumps generated in tests from **`./integration_tests/src/tests`**.
  Useful to verify if new tests generated correctly or to see what is wrong when test is failing.
- `generate_rtp_from_file` - helper to generate new input files that can be used for new tests in
  **`./integration_tests/src/tests`**.

#### `generate` create

This is crate when we keep most of our utils that generate something in repo:

- `generate_docs_examples` - generates WEBP files with examples for documentation.
- `generate_docs_example_inputs` - helper for `generate_docs_examples` (generates inputs for that binary).
- `generate_from_types` - generate JSON schema from `compositor_api` types. It needs to be called every
  time API is changed and regenerated file needs to be committed. (It also generates Markdown docs, but
  this will be soon removed)
- `generate_playground_inputs` - TODO

### Examples

TODO

### Tests

We have 3 main types of test:
- Small amount of regular unit tests spread over codebase.
- Snapshot tests that render specific image and save them as PNG.
  - Generated images are located in **`./snapshot_tests/snapshots/render_snapshots`** directory.
  - JSON files representing tested scenes are in **`./snapshot_tests`** directory
  - Actual Rust tests are in **`./src/snapshot_tests`**.
  - For example:
    - Test is located in **`./src/snapshot_tests/view_tests.rs`**
    - Test is rendering scene described by **`./snapshot_tests/view/border_radius.scene.json`**
    - Test is rendering output (or comparing with the old version) to
      **`./snapshot_tests/snapshots/render_snapshots/view/border_radius_0_output_1.png`**
- End-to-end tests that are basically snapshot tests, but compare entire videos.
  - Generated stream dumps are located in **`./snapshot_tests/snapshots/rtp_packet_dumps`** directory.
  - Tests are implemented in **`./integration_tests/src/tests`** directory.
  - Some of the test here might be fragile if running along a lot of CPU intensive
    tasks. That is why **`./.config/nextest.toml`** is limiting concurrency of some tests on CI.

Initially when setting up the repo, or when checking out different branch you need to update
submodule to the correct version by running
```
git submodule update --init --checkout
```

> Carefully read output of the command to make sure it worked. It will fail if you have uncommitted
changes in the **`./snapshot_tests/snapshots`** directory.

To run all test run:

```
cargo nextest run --workspace --profile ci
```

If you don't have `nextest` installed you can add it with;

```
cargo install cargo-nextest
```

To run specific test add name of the function or crate at the end:

e.g.

```
cargo nextest run --workspace audio_mixing_with_offset
```

will run a test from **`./integration_tests/src/tests/audio_only.rs`**

#### Updating snapshots

By default, snapshot tests generate new output and compare with the old version committed
to repo. If version in repo is different or missing then tests will fail.

To generate snapshots for new tests, or update those that should change you need to enable
`update_snapshots` feature when running tests. It is recommended to only run that command
for specific tests, especially **`./integration_tests/src/tests`**

e.g.

```
cargo nextest run --workspace --features update_snapshots audio_mixing_with_offset
```

will run a test from **./integration_tests/src/tests/audio_only.rs** and if output changed the update the snapshot.

If you made changes that modify the snapshot:
- Create PR in https://github.com/membraneframework-labs/video_compositor_snapshot_tests repo.
- Create PR in live compositor repo with link to the snapshot repo PR.
- Merge them together.

## TypeScript SDK

### Packages

- `@swmansion/smelter` - React components and common API types. (analog of `react` package).
- `@swmansion/smelter-core` - Base implementation that is used by packages like `@swmansion/smelter-node`.
- `@swmansion/smelter-node` - Node.js SDK for compositor  (analog of `react-dom` for Node.js)
- `@swmansion/smelter-web-wasm` - Browser SDK for compositor that includes compositor compiled to WASM (analog of `react-dom` for Node.js)
- `@swmansion/smelter-browser-render` - Rendering engine from Smelter compiled to WASM.
  - Run `pnpm run build-wasm` to build WASM bundle from Rust code
  - Run `pnpm run build` to build JS code (WASM has to be build earlier, or use `build:all` in root directory to build everything)

### API compatibility

> WARNING this is the goal, it is currently not true.

SDK should be always compatible with version of code from the repo. If you are changing API the changes to SDK should land in the same PR.

### Examples

To bootstrap repo run:

```
pnpm install && pnpm build:all
```

After that you can run only `pnpm run build` to rebuild if Rust code did not change.

#### Node.js: **`./ts/examples/node-examples/`**

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

#### Web WASM: **`./ts/examples/vite-browser-render`**

Run

```
pnpm run dev
```

Open `localhost:5173` in the browser

If Rust code changes you need to rebuild WASM with `pnpm run build-wasm`.

## Process for introducing API changes

Everything in the same PR.

- Update Rust code.
- Run `cargo run --bin generate_from_types` in **`./generate`** that will generate **`./schemas/scene.schema.json`**.
- Run `pnpm run generate-types` in **`./ts`** that will generate **`./ts/smelter/src/api.generated.ts`**.
- Update TypeScript code to support new changes.
- Update CHANGELOG

> To avoid problems with forgetting about adding some changes to TS, everything that shows up in PR diff for
  `./ts/smelter/src/api.generated.ts` should be addressed in the PR that regenerated those types.
