# Smelter Python SDK

Python workspace with two packages:

- **smelter-sdk** — Client library for interacting with Smelter.
- **example** — Example usage of smelter-sdk.

## Setup

```bash
cd sdks/python
uv sync
```

## Running

```bash
uv run example <optional_example.mp4>
```

By default, it detects people on the video, but you can change it e.g. 

```bash
DETECT_CLASSES=car,truck uv run example ../../../cars.mp4
```

### Streaming camera

If you did not provide mp4 file you can stream your webcam by going to:

[https://smelter-labs.github.io/tools/#whip-streamer?url=http://127.0.0.1:9000/whip/input&token=example](https://smelter-labs.github.io/tools/#whip-streamer?url=http://127.0.0.1:9000/whip/input&token=example)

### Output stream

To watch composed output stream go to:

[https://smelter-labs.github.io/tools/#whep-player?url=http://127.0.0.1:9000/whep/output&token=example](https://smelter-labs.github.io/tools/#whep-player?url=http://127.0.0.1:9000/whep/output&token=example)

