# Changelog

## Unreleased

### 💥 Breaking changes
- `EncoderParameters` has extra fields:
  - Field which determines whether stream parameters are inlined in the output stream.
  - Color space and color range.
- Changed adapter and device creation API.
- `Frame<T>` has been split into `InputFrame<T>` (for encoding) and `OutputFrame<T>` (for decoding). Decoded frames now include color space and color range information.
- Renamed feature flags: `expose_parsers` -> `expose-parsers`, `vk_api_dump` -> `vk-api-dump`, `vk_validation` -> `vk-validation`.
- Removed `mark_missing_data` from decoders in favor of `process_event` method.
- Updated `wgpu` to `29.0.0`

### ✨ New features
- One-to-many transcoders via `VulkanDevice::create_transcoder` (needs `transcoder` feature enabled)
- Made `wgpu` dependency optional via `wgpu` feature (enabled by default)
- Added helpers for NV12 <-> RGBA wgpu texture conversion
- Added `DecoderEvent::SignalFrameEnd` event to make it possible to decode frames early without waiting for the next frame to arrive

### 🐛 Bug fixes
- Fix graphical bugs when the decoded bitstream switches to a lower resolution mid-stream

## [v0.2.1](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.1)

### 🐛 Bug fixes
- Fix `vkBindVideoSessionMemoryKHR` validation errors on Mesa drivers

## [v0.2.0](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.0)

### 💥 Breaking changes
- Most APIs have been refactored

### ✨ New features
- H.264 Encoding

### 🐛 Bug fixes
