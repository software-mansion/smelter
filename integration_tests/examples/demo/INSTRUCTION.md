# Interactive Example

## Start

1. Open 2 terminal windows.
2. From `integration_tests` crate
  - Start server: `cargo run --example demo_server`
  - Start client: `cargo run --example demo`

## Enviromental variables

### `HLS_INPUT_URL`

Source of `HLS` input stream.

### `MP4_INPUT_SOURCE`

Path or URL pointing to an `mp4` file. Path should be absolute or relative to current directory.

### `MP4_OUTPUT_PATH`

Path at which `mp4` output file should be saved.

### `RTP_INPUT_PATH`

Path to file that will be used as source of `RTP` stream.

### `WHIP_INPUT_BEARER_TOKEN`

Sets the bearer token to be used when connecting to the `WHIP` server provided by Smelter.

### `WHIP_OUTPUT_BEARER_TOKEN`

Bearer token used to connect to the `WHIP` server that Smelter streams to.

### `WHIP_OUTPUT_URL`

URL of the `WHIP` server that Smelter should connect to.

### `WHEP_OUTPUT_BEARER_TOKEN`

Sets the bearer token to be used when connecting to the `WHEP` server provided by Smelter.

### `DEMO_JSON`

Path to `JSON` file that contains dump with the demo state.
