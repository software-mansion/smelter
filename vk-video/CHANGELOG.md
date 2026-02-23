# Changelog

## Unreleased

### 💥 Breaking changes
- `EncoderParameters` has an extra field, which determines whether stream parameters are inlined in the output stream.
- Changed adapter and device creation API


### ✨ New features
- One-to-many transcoders via `VulkanDevice::create_transcoder`

### 🐛 Bug fixes

## [v0.2.1](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.1)

### 🐛 Bug fixes
- Fix `vkBindVideoSessionMemoryKHR` validation errors on Mesa drivers

## [v0.2.0](https://github.com/software-mansion/smelter/releases/tag/vk-video%2Fv0.2.0)

### 💥 Breaking changes
- Most APIs have been refactored

### ✨ New features
- H.264 Encoding

### 🐛 Bug fixes
