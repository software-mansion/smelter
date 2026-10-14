# Changelog

## Unreleased

### ‼️ Major changes
#### Asynchronous API
We've long known that gpu-video's performance is limited by the synchronous nature of its API.
The GPU could do more work, but instead the CPU waits for previous work to finish before scheduling more.
With this release, gpu-video can finally work asynchronously, keeping the GPU busy. Decoders, encoders and transcoders
now receive results via a callback passed at creation:
```rs
let mut decoder = device.create_wgpu_textures_decoder_h264(
    &queue,
    gpu_video::parameters::DecoderParameters::default(),
    |decoded_frame| {
        // ...
    },
).unwrap();
```
The callbacks should only do lightweight work, such as passing the results to another thread. Depending on the backend, all callbacks run on one shared thread, so heavy work done in one callback can block all other callbacks.

By @noituri ([#2143](https://github.com/software-mansion/smelter/pull/2143), [#2215](https://github.com/software-mansion/smelter/pull/2215), [#2239](https://github.com/software-mansion/smelter/pull/2239), [#2241](https://github.com/software-mansion/smelter/pull/2241), [#2251](https://github.com/software-mansion/smelter/pull/2251) and [#2264](https://github.com/software-mansion/smelter/pull/2264))

#### macOS support
Vulkan is no longer the only supported backend. We have reworked the internal APIs to make room for new backends, which allowed us to add support for macOS
via VideoToolbox. Many types have been renamed to reflect that change (the `Vulkan` prefix has been replaced with `Video`):
- `VulkanInstance` -> `VideoInstance`
- `VulkanAdapter` -> `VideoAdapter`
- `VulkanDevice` -> `VideoDevice`
- etc

macOS support by @jerzywilczek ([#2186](https://github.com/software-mansion/smelter/pull/2186), [#2187](https://github.com/software-mansion/smelter/pull/2187) and [#2214](https://github.com/software-mansion/smelter/pull/2214)). \
Backend-agnostic API by @noituri ([#2090](https://github.com/software-mansion/smelter/pull/2090), [#2091](https://github.com/software-mansion/smelter/pull/2091), [#2100](https://github.com/software-mansion/smelter/pull/2100), [#2103](https://github.com/software-mansion/smelter/pull/2103) and [#2107](https://github.com/software-mansion/smelter/pull/2107)).

#### New wgpu-based initialization
Previously, `wgpu::Device` and `wgpu::Queue` could only be retrieved after `VulkanDevice` was created. The new approach follows the typical
wgpu initialization process, but instead of `wgpu::Adapter::request_device` you use `gpu_video::VideoAdapterExt::request_device_with_video_support`.
Decoders, encoders and transcoders are created with `VideoDevice`, which is retrieved from `wgpu::Device` via `gpu_video::VideoDeviceExt::video`.
When wgpu is not used, initialization is done via `VideoInstance` (previously `VulkanInstance`).

```rs
use gpu_video::{VideoAdapterExt, VideoDeviceExt, parameters::VideoDeviceDescriptor};

let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
let adapter = instance.enumerate_adapters(Default::default())
    .await
    .into_iter()
    .find(|a| {
        a.video_adapter_info()
            .is_some_and(|info| info.decode_capabilities.h264.is_some())
    })
    .unwrap();
let (device, queue) = adapter
    .request_device_with_video_support(&VideoDeviceDescriptor::default())
    .unwrap();

let video_device = device.video().unwrap();
// use video_device to create decoders/encoders/transcoders...
```

By @noituri ([#2039](https://github.com/software-mansion/smelter/pull/2039))

### 💥 Breaking changes
- Updated `wgpu` to `30.0.0` ([#2111](https://github.com/software-mansion/smelter/pull/2111) by @noituri)
- Removed `vk-validation` and `vk-api-dump` features. Instead you can enable them when creating a `VideoInstance` or `wgpu::Instance` ([#2039](https://github.com/software-mansion/smelter/pull/2039) by @noituri)
- Removed `EncodeProfileCapabilities::quality_levels` ([#2145](https://github.com/software-mansion/smelter/pull/2145) by @noituri)
- Encoders now take `InputFrame<RawFrameRef>`s as input. `RawFrameRef` is similar to `RawFrameData`, but it borrows the contents. ([#2244](https://github.com/software-mansion/smelter/pull/2244) by @jerzywilczek)
- wgpu decoders and encoders exchange frames as `VideoTexture` instead of `wgpu::Texture`, which lets NV12 frames work on Metal ([#2245](https://github.com/software-mansion/smelter/pull/2245) by @jerzywilczek)

### ✨ New features
- Added VUI support to H.265 encoder ([#1995](https://github.com/software-mansion/smelter/pull/1995) by @noituri)
- Improved encoder performance on Vulkan by using a better synchronisation approach ([#2269](https://github.com/software-mansion/smelter/pull/2269) by @noituri)

### 🐛 Bug fixes
- Request only the transfer and compute queues that are actually used when creating the device. ([#2223](https://github.com/software-mansion/smelter/pull/2223) by @jerzywilczek)
- H.264 decoder now hands the whole DPB to the Vulkan driver and lets it build the reference picture lists. ([#2219](https://github.com/software-mansion/smelter/pull/2219) by @jerzywilczek)
- Fix artifacts in h264 decoder caused by unnecessary short reference deletion ([#1991](https://github.com/software-mansion/smelter/pull/1991) by @noituri)
- `bytemuck` is now only required if compiling with the `transcoder` feature ([#1986](https://github.com/software-mansion/smelter/pull/1986) by @jerzywilczek)
- Fix "green bar" artifact in H.264 decoder which appeared on Mesa drivers ([#2071](https://github.com/software-mansion/smelter/pull/2071) by @noituri)
- Fix decoding video with a dynamic resolution ([#2080](https://github.com/software-mansion/smelter/pull/2080) by @noituri)
- Fix vulkan validation errors produced by the encoder ([#2178](https://github.com/software-mansion/smelter/pull/2178) by @noituri)
- Fix artifacts on AMD GPUs on Windows when multiple encoders or decoders are used at the same time ([#2249](https://github.com/software-mansion/smelter/pull/2249) by @noituri)
- Fix the RGBA to NV12 conversion shader to output limited range NV12 ([#2050](https://github.com/software-mansion/smelter/pull/2050) by @jerzywilczek)
- Fix device feature collisions when gpu-video is used with wgpu ([#2255](https://github.com/software-mansion/smelter/pull/2255) by @noituri)

## [v0.4.0](https://github.com/software-mansion/smelter/releases/tag/gpu-video%2Fv0.4.0)

### 💥 Breaking changes
- Removed `ReferenceManagementError::GapsInFrameNumNotSupported`. H.264 streams with `gaps_in_frame_num_value_allowed_flag = 1` should decode correctly now, unless they include non-existing references in the reference list after the modification process. Then they hit the newly added `ReferenceManagementError::NonExistingReferenceInActiveList`. ([#1957](https://github.com/software-mansion/smelter/pull/1957), [#1972](https://github.com/software-mansion/smelter/pull/1972) by @jerzywilczek)
- All nalus returned by `H264Parser` now contain their own start codes (`001` or `0001` bytes at the beginning) ([#1921](https://github.com/software-mansion/smelter/pull/1921) by @noituri)
- Decoders, encoders and encoder parameters are now created using codec-specific methods, e. g. `Device::encoder_output_parameters_low_latency` -> `Device::encoder_output_parameters_h264_low_latency`, `Device::create_bytes_encoder` -> `Device::create_bytes_encoder_h264` ([#1871](https://github.com/software-mansion/smelter/pull/1871) by @jerzywilczek)
- Added `DecoderEvent::DecodeParsedFrame` event for decoding already parsed frames (needs `expose-parsers` feature enabled). The type of the frame is provided as a generic type of `DecoderEvent`. Users using H.264 decoders should switch to `H264DecoderEvent` alias ([#1936](https://github.com/software-mansion/smelter/pull/1936) by @noituri)
- Renamed the library to `gpu-video` ([#1942](https://github.com/software-mansion/smelter/pull/1942) by @jerzywilczek)

### ✨ New features
- Added an H.265 encoder ([#1919](https://github.com/software-mansion/smelter/pull/1919) by @jerzywilczek)

### 🐛 Bug fixes
- Use Box in QueueCreateInfo ([#1938](https://github.com/software-mansion/smelter/pull/1938) by @krakow10)
- Use Box instead of NonNull in h264 & h265 codec parameters ([#1934](https://github.com/software-mansion/smelter/pull/1934) by @krakow10)

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
