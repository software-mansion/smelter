# vk-video

A library for hardware video decoding (and soon encoding) using Vulkan Video, with [wgpu] integration.

[![Crates.io][crates-badge]][crates-url]
[![docs.rs][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/vk-video
[crates-url]: https://crates.io/crates/vk-video
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/software-mansion/smelter/blob/master/vk-video/LICENSE
[docs-badge]: https://img.shields.io/docsrs/vk-video
[docs-url]: https://docs.rs/vk-video/latest/vk_video/

## Overview

The goal of this library is to provide easy access to hardware video coding. You can use it to decode a video frame into a `Vec<u8>` with pixel data, or into a [`wgpu::Texture`]. Currently, we only support H.264 (aka AVC or MPEG 4 Part 10) decoding, but we plan to support at least H.264 encoding and hopefully other codecs supported by Vulkan Video.

An advantage of using this library with wgpu is that decoded video frames never leave the GPU memory. There's no copying the frames to RAM and back to the GPU, so it should be quite fast if you want to use them for rendering.

This library was developed as a part of [smelter, a tool for video composition](https://smelter.dev/).

## Usage

```rs
fn decode_video(
    window: &winit::window::Window,
    mut encoded_video_reader: impl std::io::Read,
) {
    let instance = vk_video::VulkanInstance::new().unwrap();
    let surface = instance.wgpu_instance.create_surface(window).unwrap();
    let device = instance
        .create_device(
            wgpu::Features::empty(),
            wgpu::Limits::default(),
            Some(&surface),
        )
        .unwrap();

    let mut decoder = device.create_wgpu_textures_decoder().unwrap();
    let mut buffer = vec![0; 4096];

    while let Ok(n) = encoded_video_reader.read(&mut buffer) {
        if n == 0 {
            return;
        }

        let decoded_frames = decoder.decode(&buffer[..n], None).unwrap();

        for frame in decoded_frames {
            // Each frame contains a wgpu::Texture you can sample for drawing.
            // device.wgpu_device() will give you a wgpu::Device and device.wgpu_queue()
            // a wgpu::Queue. You can use these for interacting with the frames.
        }
    }
}
```
Be sure to check out our examples, especially the `player` example, which is a simple video player built using this library and wgpu. Because the player is very simple, you need to extract the raw h264 data from a container before usage. Here's an example on how to extract the h264 bytestream out of an mp4 file using ffmpeg:

```sh
ffmpeg -i input.mp4 -c:v copy -bsf:v h264_mp4toannexb -an output.h264
```

Then you can run the example with:

```sh
git clone https://github.com/software-mansion/smelter.git
cd smelter/vk-video
cargo run --example player -- output.h264 FRAMERATE
```

## Compatibility

On Linux, the library should work on NVIDIA and AMD GPUs out of the box with recent Mesa drivers. For AMD GPUs with a bit older Mesa drivers, you may need to set the `RADV_PERFTEST=video_decode` environment variable:

```sh
RADV_PERFTEST=video_decode cargo run
```

It should work on Windows with recent drivers out of the box. Be sure to submit an issue if it doesn't.

[wgpu]: https://wgpu.rs/
[`wgpu::Texture`]: https://docs.rs/wgpu/latest/wgpu/struct.Texture.html

## vk-video is created by Software Mansion

[![swm](https://logo.swmansion.com/logo?color=white&variant=desktop&width=150&tag=smelter-vk-video 'Software Mansion')](https://swmansion.com)

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter-vk-video&utm_medium=readme).
