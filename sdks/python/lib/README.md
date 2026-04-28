# smelter-sdk

Python client for [Smelter](https://smelter.dev) — a real-time, low-latency,
programmable video and audio composition toolkit.

The SDK currently exposes Smelter's **side channel**: a Unix-socket stream of
decoded RGBA video frames and PCM audio batches, so Python code can run ML
inference (YOLO, Whisper, …) against live media and feed results back into
the running pipeline via Smelter's HTTP API.

> **Status:** alpha. The API may change between 0.x releases.

## Install

```bash
pip install smelter-sdk
```

Requires Python 3.11+ and NumPy 1.26+. Tested on Linux and macOS. The Smelter
server itself is a separate Rust binary — see the
[Smelter docs](https://smelter.dev/docs) for installation.

## Quickstart (sync)

The SDK reads its socket directory from the
`SMELTER_SIDE_CHANNEL_SOCKET_DIR` environment variable (the same one the
server publishes), falling back to the current working directory. With that
exported, the API takes one positional argument — the Smelter input id:

```python
from smelter import subscribe_video_channel

for frame in subscribe_video_channel("cam1"):
    # frame.rgba is a writable np.ndarray, shape (H, W, 4), dtype uint8.
    # frame.pts_nanos is a presentation timestamp in nanoseconds.
    process(frame.rgba, frame.pts_nanos)
```

Audio:

```python
from smelter import subscribe_audio_channel

for batch in subscribe_audio_channel("cam1"):
    # batch.samples is a (sample_count, channels) np.ndarray, dtype float32.
    mono = batch.to_mono()                 # shape (sample_count,)
    print(batch.start_pts_seconds, batch.duration_seconds)
```

If the env var isn't right for your process, pass an explicit
[`Context`](#context):

```python
from smelter import Context, subscribe_video_channel

ctx = Context(socket_dir="/var/run/smelter")
for frame in subscribe_video_channel("cam1", ctx=ctx, timeout=10):
    ...
```

## Quickstart (asyncio)

```python
import asyncio
from smelter.aio import subscribe_video_channel

async def main():
    async for frame in subscribe_video_channel("cam1"):
        await process(frame.rgba, frame.pts_nanos)

asyncio.run(main())
```

The async client uses `asyncio.open_unix_connection` directly — no thread-pool
wrapper. Heavy synchronous work inside the loop (e.g. ML inference) should
still be wrapped in `asyncio.to_thread(...)` to avoid blocking other tasks.

## Context

A `Context` bundles SDK-wide configuration. Today it carries one field — the
socket directory — but is the natural extension point for future options
(default timeouts, HTTP API base URL, …).

```python
Context()                                  # env var, then cwd
Context(socket_dir="/var/run/smelter")     # explicit
```

Every public entry point (`subscribe_video_channel`, `subscribe_audio_channel`, `list_channels`,
`wait_for_channel`, …) accepts an optional `ctx=` keyword. When omitted, a
fresh default context is constructed per call.

## Lower-level API

The top-level `smelter` module exposes the recommended high-level surface.
Lower-level building blocks live in `smelter.sync` (and `smelter.aio` for the
async equivalent) — reach for them only when the one-call
`subscribe_*_channel` helpers don't fit:

```python
from smelter import SideChannelKind, list_channels
from smelter.sync import connect_video, wait_for_channel

# Enumerate everything currently published.
for info in list_channels():
    print(info.kind, info.input_id)

# Wait for a specific channel, then connect.
info = wait_for_channel(kind=SideChannelKind.VIDEO, input_id="cam1", timeout=10)
with connect_video(info, timeout=5.0) as conn:
    frame = conn.recv()           # raises ConnectionClosed / RecvTimeout
    for frame in conn:            # iterates until peer closes
        ...
```

`smelter.sync.connect_audio(info, dtype=np.float64)` keeps full f64 precision;
the default is `float32` to match Whisper / opus / librosa.

## Errors

All exceptions inherit from `smelter.SmelterError`:

| Exception            | Raised when                                                |
|----------------------|------------------------------------------------------------|
| `ConnectionClosed`   | Peer closed the socket — normal end-of-stream.             |
| `RecvTimeout`        | `recv()` exceeded the configured timeout.                  |
| `ChannelNotFound`    | `wait_for_channel` timed out before a socket appeared.     |
| `ProtocolError`      | A message did not match the expected wire format.          |

`ChannelNotFound` and `RecvTimeout` also inherit from the built-in `TimeoutError`.

## Stability

The SDK is alpha — any 0.x release may break compatibility. Once 1.0 ships,
the following surface will be governed by semver:

- Everything re-exported from the top-level `smelter` package (see
  `smelter.__all__`) and `smelter.aio` (`smelter.aio.__all__`).
- The synchronous building blocks in `smelter.sync` (`connect_video`,
  `connect_audio`, `wait_for_channel`, the `VideoConnection` / `AudioConnection`
  classes) and their `smelter.aio` equivalents.
- The data types in `smelter.types` (`VideoFrame`, `AudioBatch`,
  `SideChannelInfo`, `SideChannelKind`) and the exceptions in `smelter.errors`.

Anything starting with an underscore (`smelter._protocol`, `smelter._discovery`)
is internal and may change without notice.

## What's not in the SDK (yet)

- A typed client for the Smelter HTTP API. To send scene updates back to
  Smelter today, use `requests` / `httpx` against the `/api/output/.../update`
  endpoint directly.
- A way to push Python-generated frames into Smelter as an input. The side
  channel is unidirectional (Smelter → Python) at the moment.

## Examples

The repository ships a complete demo that wires a video input through YOLO
object detection and Whisper speech-to-text, then renders the boxes and
subtitles back over the stream. See
[`sdks/python/example/`](https://github.com/software-mansion/smelter/tree/master/sdks/python/example).

## License

MIT — see [LICENSE](LICENSE).
