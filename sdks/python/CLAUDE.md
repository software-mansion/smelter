# Python SDK

Python bindings for smelter plus the `example/` app that drives the server with transcription and YOLO detection.

## Layout

- `lib/` — Python bindings (`smelter` package): side-channel client, types.
- `example/` — Runnable demo wiring transcription + detection into smelter scene updates.
- `pyproject.toml` / `uv.lock` — workspace managed with `uv`.

## Checks

Always run these after editing any Python file in this directory and fix anything they report:

```
uv run ruff check
uv run ruff format
uv run ty check
```
