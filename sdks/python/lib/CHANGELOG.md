# Changelog

All notable changes to `smelter-sdk` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

Initial release.

- Side-channel client: subscribe to per-input video (RGBA) and audio (PCM) streams over unix sockets.
- Synchronous API (`smelter.subscribe_video_channel`, `smelter.subscribe_audio_channel`, `smelter.list_channels`).
- Asyncio API (`smelter.aio`).
- `Context` for explicit socket-directory configuration.
- Typed exceptions (`SmelterError` and subclasses).
