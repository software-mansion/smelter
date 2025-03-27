# Changelog

## unreleased

### 💥 Breaking changes

### ✨ New features

### 🐛 Bug fixes

- Fix web renderer crashing when multiple Smelter instances are run. ([#1016](https://github.com/software-mansion/smelter/pull/1016) by [@noituri](https://github.com/noituri))

### 🔧 Others

- Deprecate `vulkan_video` option in favor of `vulkan_h264`. ([#1032](https://github.com/software-mansion/live-compositor/pull/1032) by [@wkozyra95](https://github.com/wkozyra95))
- Deprecate `video_decoder` option in MP4 input. ([#1032](https://github.com/software-mansion/live-compositor/pull/1032) by [@wkozyra95](https://github.com/wkozyra95))

## [v0.4.1](https://github.com/software-mansion/live-compositor/releases/tag/v0.4.1)

### 🐛 Bug fixes

- Fix DeckLink color tint (green tint on output). ([#1000](https://github.com/software-mansion/live-compositor/pull/1000) by [@wkozyra95](https://github.com/wkozyra95))
- Fix AAC codec information in MP4 output. ([#998](https://github.com/software-mansion/live-compositor/pull/998), [#999](https://github.com/software-mansion/live-compositor/pull/999) by [@wkozyra95](https://github.com/wkozyra95))

## [v0.4.0](https://github.com/software-mansion/live-compositor/releases/tag/v0.4.0)

### 💥 Breaking changes

- Drop support for `SHADER_UNUSED_VERTEX_OUTPUT` `wgpu` feature.  ([#733](https://github.com/software-mansion/live-compositor/pull/733) by [@jerzywilczek](https://github.com/jerzywilczek))
- Rename component properties describing color. Remove `_rgba` suffix. ([#896](https://github.com/software-mansion/live-compositor/issues/896) by [@BrtqKr](https://github.com/BrtqKr))
- Replace the `LIVE_COMPOSITOR_OUTPUT_SAMPLE_RATE` configuration environment variable with `LIVE_COMPOSITOR_MIXING_SAMPLE_RATE`. The output sample rate is now determined using encoder options on `register_output`. Change the default output sample rate for AAC codec to 44100 Hz. ([#925](https://github.com/software-mansion/live-compositor/pull/925) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Change prefix of all environment variables `LIVE_COMPOSITOR_` → `SMELTER_`. ([#941](https://github.com/software-mansion/live-compositor/pull/941) by [@wkozyra95](https://github.com/wkozyra95))

### ✨ New features

- Add `loop` option for MP4 input. ([#699](https://github.com/software-mansion/live-compositor/pull/699) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Add `LIVE_COMPOSITOR_LOG_FILE` environment variable to enable logging to file ([#853](https://github.com/software-mansion/live-compositor/pull/853) by [@wkozyra95](https://github.com/wkozyra95))
- Add border, border radius and box shadow options to `Rescaler` and `View` components. ([#815](https://github.com/software-mansion/live-compositor/pull/815) by [@WojciechBarczynski](https://github.com/WojciechBarczynski)), ([#839](https://github.com/software-mansion/live-compositor/pull/839), [#842](https://github.com/software-mansion/live-compositor/pull/842), [#858](https://github.com/software-mansion/live-compositor/pull/858) by [@wkozyra95](https://github.com/wkozyra95))
- Extend supported color formats. ([#896](https://github.com/software-mansion/live-compositor/issues/896) by [@BrtqKr](https://github.com/BrtqKr))
- Allow specifying output sample rates per output in `register_output` requests. ([#925](https://github.com/software-mansion/live-compositor/pull/925) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Add WHIP server as an input. ([#881](https://github.com/software-mansion/live-compositor/pull/881) by [@wkazmierczak](https://github.com/wkazmierczak))
- Add WHIP client as an output. ([#834](https://github.com/software-mansion/live-compositor/pull/834) by [@wkazmierczak](https://github.com/wkazmierczak), [@brzep](https://github.com/brzep))
- Add Vulkan based hardware decoder. ([#803](https://github.com/software-mansion/live-compositor/pull/803), [#875](https://github.com/software-mansion/live-compositor/pull/875) by [@jerzywilczek](https://github.com/jerzywilczek))
- Add padding options to `View` component. ([#931](https://github.com/software-mansion/live-compositor/pull/931) by [@noituri](https://github.com/noituri))

### 🐛 Bug fixes

- Fix AAC output unregister before the first sample. ([#714](https://github.com/software-mansion/live-compositor/pull/714) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Fix output mp4 timestamps when output is registered after pipeline start. ([#731](https://github.com/software-mansion/live-compositor/pull/731) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))

### 🔧 Others

- Automatically rename file under the output path for MP4 output if it already exists. ([#684](https://github.com/software-mansion/live-compositor/pull/684) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Make `video.encoder.preset` optional in the output register. ([#782](https://github.com/software-mansion/live-compositor/pull/782) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Use FFmpeg option `-movflags faststart` when creating **\*.mp4** files. ([#807](https://github.com/software-mansion/live-compositor/pull/807) by [@jerzywilczek](https://github.com/jerzywilczek))
- Return **\*.mp4** duration when registering inputs. ([#890](https://github.com/software-mansion/live-compositor/pull/890) by [@wkozyra95](https://github.com/wkozyra95))

## [v0.3.0](https://github.com/software-mansion/live-compositor/releases/tag/v0.3.0)

### 💥 Breaking changes

- Remove `forward_error_correction` option from RTP OPUS output. ([#615](https://github.com/software-mansion/live-compositor/pull/615) by [@wkozyra95](https://github.com/wkozyra95))

### ✨ New features

- Support DeckLink cards as an input. ([#587](https://github.com/software-mansion/live-compositor/pull/587), [#597](https://github.com/software-mansion/live-compositor/pull/597), [#598](https://github.com/software-mansion/live-compositor/pull/598), [#599](https://github.com/software-mansion/live-compositor/pull/599) by [@wkozyra95](https://github.com/wkozyra95))
- Add `LIVE_COMPOSITOR_INPUT_BUFFER_DURATION_MS` environment variable to control input stream buffer size. ([#600](https://github.com/software-mansion/live-compositor/pull/600) by [@wkozyra95](https://github.com/wkozyra95))
- Add endpoint for requesting keyframe on the output stream. ([#620](https://github.com/software-mansion/live-compositor/pull/620) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Add MP4 output ([#657](https://github.com/software-mansion/live-compositor/pull/657) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))
- Add `OUTPUT_DONE` WebSocket event ([#658](https://github.com/software-mansion/live-compositor/pull/658) by [@WojciechBarczynski](https://github.com/WojciechBarczynski))

### 🐛 Bug fixes

- Fix input queueing when some of the inputs do not produce frames/samples . ([#625](https://github.com/software-mansion/live-compositor/pull/625) by [@wkozyra95](https://github.com/wkozyra95))

## [v0.2.0](https://github.com/software-mansion/live-compositor/releases/tag/v0.2.0)

Initial release
