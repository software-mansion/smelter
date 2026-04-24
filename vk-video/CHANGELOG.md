# Changelog

## Unreleased

### 💥 Breaking changes
- All nalus returned by `H264Parser` now contain their own start codes (`001` or `0001` bytes at the beginning) ([#1921](https://github.com/software-mansion/smelter/pull/1921) by @noituri)

- Decoders, encoders and encoder parameters are now created using codec-specific methods, e. g. `Device::encoder_output_parameters_low_latency` -> `Device::encoder_output_parameters_h264_low_latency`, `Device::create_bytes_encoder` -> `Device::create_bytes_encoder_h264` ([#1871](https://github.com/software-mansion/smelter/pull/1871) by @jerzywilczek)

### ✨ New features

- Added an H.265 encoder ([#1919](https://github.com/software-mansion/smelter/pull/1919) by @jerzywilczek)

### 🐛 Bug fixes

## [v0.3.0](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.3.0)

### 💥 Breaking changes
- `EncoderParameters` had its structure changed, introducing `EncoderOutputParameters` as one of the fields ([#1865](https://github.com/software-mansion/smelter/pull/1865) by @jerzywilczek)
- New `EncoderOutputParameters` type (split from `EncoderParameters`) adds fields for:
  - Determining whether stream parameters are inlined in the output stream. ([#1775](https://github.com/software-mansion/smelter/pull/1775) by @jerzywilczek)
  - Color space and color range. ([#1806](https://github.com/software-mansion/smelter/pull/1806) by @noituri)
- Renamed `Device::encoder_parameters_high_quality` to `Device::encoder_output_parameters_high_quality` and `Device::encoder_parameters_low_latency` to `Device::encoder_output_parameters_low_latency` ([#1869](https://github.com/software-mansion/smelter/pull/1869) by @jerzywilczek)
- Changed adapter and device creation API. ([#1756](https://github.com/software-mansion/smelter/pull/1756) by @noituri)
- `Frame<T>` has been split into `InputFrame<T>` (for encoding) and `OutputFrame<T>` (for decoding). Decoded frames now include color space and color range information. ([#1831](https://github.com/software-mansion/smelter/pull/1831) by @noituri)
- Renamed feature flags: `expose_parsers` -> `expose-parsers`, `vk_api_dump` -> `vk-api-dump`, `vk_validation` -> `vk-validation`. ([#1849](https://github.com/software-mansion/smelter/pull/1849) by @wkozyra95)
- Removed `mark_missing_data` from decoders in favor of `process_event` method. ([#1854](https://github.com/software-mansion/smelter/pull/1854) by @noituri)
- Updated `wgpu` to `29.0.0` ([#1859](https://github.com/software-mansion/smelter/pull/1859) by @noituri)

### ✨ New features
- One-to-many transcoders via `VulkanDevice::create_transcoder` (needs `transcoder` feature enabled) ([#1769](https://github.com/software-mansion/smelter/pull/1769) by @jerzywilczek, [#1823](https://github.com/software-mansion/smelter/pull/1823) by @noituri, [#1846](https://github.com/software-mansion/smelter/pull/1846) by @jerzywilczek)
- Made `wgpu` dependency optional via `wgpu` feature (enabled by default) ([#1756](https://github.com/software-mansion/smelter/pull/1756) by @noituri)
- Added helpers for NV12 <-> RGBA wgpu texture conversion ([#1736](https://github.com/software-mansion/smelter/pull/1736) by @noituri)
- Added `DecoderEvent::SignalFrameEnd` event to make it possible to decode frames early without waiting for the next frame to arrive ([#1854](https://github.com/software-mansion/smelter/pull/1854) by @noituri)
- Encoder API is now safe ([#1863](https://github.com/software-mansion/smelter/pull/1863) by @noituri)

### 🐛 Bug fixes
- Fix graphical bugs when the decoded bitstream switches to a lower resolution mid-stream ([#1787](https://github.com/software-mansion/smelter/pull/1787) by @jerzywilczek)

## [v0.2.1](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.1)

### 🐛 Bug fixes
- Fix `vkBindVideoSessionMemoryKHR` validation errors on Mesa drivers ([#1739](https://github.com/software-mansion/smelter/pull/1739) by @noituri)

## [v0.2.0](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.0)

### 💥 Breaking changes
- Most APIs have been refactored ([#1651](https://github.com/software-mansion/smelter/pull/1651) by @noituri)

### ✨ New features
- H.264 Encoding ([#1215](https://github.com/software-mansion/smelter/pull/1215) by @noituri and @jerzywilczek)

### 🐛 Bug fixes
- Use Box instead of NonNull in h264 & h265 codec parameters ([#1934](https://github.com/software-mansion/smelter/pull/1934) by @krakow10)
