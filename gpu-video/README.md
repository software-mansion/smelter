# gpu-video

A library for hardware video decoding and encoding, in Rust, with [wgpu] integration. We aim to be the simplest and most idiomatic Rust library for video processing. No FFmpeg code is used in gpu-video.

[![Crates.io][crates-badge]][crates-url]
[![docs.rs][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/gpu-video
[crates-url]: https://crates.io/crates/gpu-video
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/software-mansion/smelter/blob/master/gpu-video/LICENSE
[docs-badge]: https://img.shields.io/docsrs/gpu-video
[docs-url]: https://docs.rs/gpu-video/latest/gpu_video/

## Overview

The goal of this library is to provide easy access to hardware video coding. You can use it to decode and encode a video frame to/from `Vec<u8>` with pixel data, or straight to/from GPU textures through [wgpu]. Currently, we support the following codecs:

|            | Decode | Encode |
|:----------:|:------:|:------:|
| H.264/AVC  | ✅      | ✅      |
| H.265/HEVC | ❌      | ✅      |
| AV1        | ❌      | 🚧     |

- ✅ - should work, file issues if there are problems
- 🚧 - working on this currently
- ❌ - not supported yet, but support is planned

An advantage of using this library with wgpu is that decoded video frames never leave the GPU memory. There's no copying frames to RAM and back to the GPU, so it should be quite fast if you want to use them for rendering.

Frames are passed around as a `VideoTexture`, a thin wrapper over the [`wgpu::Texture`]s holding the [NV12](https://en.wikipedia.org/wiki/YCbCr#4:2:0) planes. It hands out plain texture views and copy descriptors for each plane, so it plugs into bind groups, render passes and copies like any other wgpu texture.

This library was developed as a part of [smelter, a tool for video composition](https://smelter.dev/).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-transparent.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-background.svg">
  <img height="60" alt="Smelter" src="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-background.svg">
</picture>

## Code samples

### Decode video frames into wgpu textures

```rust
async fn decode_video(
    mut encoded_video_reader: impl std::io::Read,
) {
    use gpu_video::{VideoAdapterExt, VideoDeviceExt, parameters::VideoDeviceDescriptor};

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .enumerate_adapters(wgpu::Backends::default())
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

    let mut decoder = video_device.create_wgpu_textures_decoder_h264(
        &queue,
        gpu_video::parameters::DecoderParameters::default(),
        |_frame| {
            // Each frame is a VideoTexture: wgpu textures with NV12 planes you can sample
            // for drawing.
            // Keep this callback light, e.g. pass the frame through a channel.
        },
    ).unwrap();

    let mut buffer = vec![0; 4096];

    while let Ok(n) = encoded_video_reader.read(&mut buffer) {
        if n == 0 {
            break;
        }

        decoder.decode(gpu_video::EncodedInputChunk {
            data: &buffer[..n],
            pts: None
        }).unwrap();
    }

    decoder.flush().unwrap();
}
```

### Encode RGBA wgpu textures

```rust
async fn encode_video(
    rgba_frame_receiver: std::sync::mpsc::Receiver<wgpu::Texture>,
) {
    use std::num::NonZeroU32;
    use gpu_video::{
        VideoAdapterExt, VideoDeviceExt, WgpuRgbaToNv12Converter,
        parameters::{ColorRange, ColorSpace, VideoDeviceDescriptor, WgpuConverterParameters},
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .enumerate_adapters(wgpu::Backends::default())
        .await
        .into_iter()
        .find(|a| {
            a.video_adapter_info()
                .is_some_and(|info| info.encode_capabilities.h264.is_some())
        })
        .unwrap();
    let (device, queue) = adapter
        .request_device_with_video_support(&VideoDeviceDescriptor::default())
        .unwrap();
    let video_device = device.video().unwrap();
    let encoder_parameters = gpu_video::parameters::EncoderParametersH264 {
        output_parameters: video_device
            .encoder_output_parameters_h264_high_quality(
                gpu_video::parameters::RateControl::VariableBitrate {
                    average_bitrate: 500_000,
                    max_bitrate: 2_000_000,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                },
            )
            .unwrap(),
        input_parameters: gpu_video::parameters::VideoParameters {
            width: NonZeroU32::new(1920).unwrap(),
            height: NonZeroU32::new(1080).unwrap(),
            target_framerate: 30.into(),
        },
        max_in_flight_submissions: None,
    };

    let mut encoder = video_device
        .create_wgpu_textures_encoder_h264(
            &queue,
            encoder_parameters,
            // callback used for passing the encoded frames
            move |chunk| {
                // ...
            }
        )
        .unwrap();

    let converter = WgpuRgbaToNv12Converter::new(
        &device,
        WgpuConverterParameters {
            color_space: ColorSpace::BT709,
            color_range: ColorRange::Limited,
        },
    )
    .unwrap();

    for rgba_frame in rgba_frame_receiver.iter() {
        // RGBA frames need `wgpu::TextureUsages::TEXTURE_BINDING`.
        let rgba_bind_group = converter.create_input_bind_group(&rgba_frame);
        let input_texture = encoder.input_texture().unwrap();
        let mut command_encoder = device.create_command_encoder(&Default::default());
        converter.convert(&mut command_encoder, &rgba_bind_group, input_texture.texture());
        queue.submit([command_encoder.finish()]);

        encoder
            .encode(
                gpu_video::InputFrame {
                    data: input_texture,
                    pts: None,
                },
                false,
            )
            .unwrap();
    }
}
```

Be sure to check out our examples, especially the `egui-player` example, which is an mp4 player built using this library, the [mp4 crate](https://crates.io/crates/mp4) and [egui](https://www.egui.rs/). You can run the example with:

```sh
git clone https://github.com/software-mansion/smelter.git
cd smelter/gpu-video/examples/egui-player
cargo run
```

## Compatibility

The library should work on Linux, Windows and macOS. It should also work on all other Apple operating systems (tvOS, iOS, watchOS etc.), but we don't test them. Be sure to submit an issue if it doesn't.

[wgpu]: https://wgpu.rs/
[`wgpu::Texture`]: https://docs.rs/wgpu/latest/wgpu/struct.Texture.html

## gpu-video is created by Software Mansion

<a href="https://swmansion.com"><img width="150" height="80" alt="Software Mansion" src="https://github.com/user-attachments/assets/cacd6185-78b0-4e76-8767-016d6389bb2b" /></a>

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter-gpu-video&utm_medium=readme).
