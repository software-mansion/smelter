# Smelter Python SDK

Python workspace for [Smelter](https://smelter.dev), with:

- **`lib/`** — [`smelter-sdk`](./lib/README.md), the published client library.
- **`example/`** — three runnable demos that exercise Smelter's side channel.

The SDK exposes Smelter's **side channel**: a Unix-socket stream of decoded
RGBA video frames and PCM audio batches, so Python code can run ML inference
(YOLO, Whisper, …) against live media and feed results back via Smelter's
HTTP API.

## Setup

```bash
cd sdks/python
uv sync
```

## Examples

### `example-simple-sync` / `example-simple-async`

Minimal side-channel subscribers — enumerate every published channel and print
one line per video frame and per audio batch. Use these as a starting point
when wiring the SDK into your own code.

Requires a Smelter server already running with `side_channel` enabled on at
least one input. Point the env var at the directory Smelter writes its sockets
into:

```bash
SMELTER_SIDE_CHANNEL_SOCKET_DIR=/path/to/sockets uv run example-simple-sync
SMELTER_SIDE_CHANNEL_SOCKET_DIR=/path/to/sockets uv run example-simple-async
```

### `example-yolo-whisper`

Full demo: boots a Smelter server, runs YOLO object detection and Whisper
speech-to-text on the input, and renders detection boxes and subtitles back
over the stream.

```bash
uv run example-yolo-whisper <optional_example.mp4>
```

By default it detects people. Override the YOLO class filter via env var:

```bash
DETECT_CLASSES=car,truck uv run example-yolo-whisper ./cars.mp4
```

#### Streaming a webcam

If you don't pass an MP4, the example registers a WHIP input. Stream your
webcam to it from:

<https://smelter-labs.github.io/tools/#whip-streamer?url=http://127.0.0.1:9000/whip/input&token=example>

#### Watching the composed output

The example exposes a WHEP endpoint with detection boxes + subtitles:

<https://smelter-labs.github.io/tools/#whep-player?url=http://127.0.0.1:9000/whep/output&token=example>

It also writes a fragmented MP4 to `/tmp/smelter_output.mp4`.
