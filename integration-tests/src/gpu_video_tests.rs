use std::path::PathBuf;

use gpu_video::{
    VideoAdapterExt,
    parameters::{ColorRange, ColorSpace, VideoDeviceDescriptor},
};

mod decode_tests;
mod encode_tests;
mod harness;

// TODO: Parser tests?
// TODO: Tests on windows
// TODO: if test fails, dump it to file so that it's viewable

struct Nv12Frame {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
}

struct TestCase<Options> {
    /// Path relative to the gpu-video dumps directory in the snapshots submodule.
    pub dump_file_path: PathBuf,
    pub options: Options,
    /// Maximum allowed error between the gpu-video output and the ffmpeg reference output.
    pub allowed_error: f32,
}

fn create_video_device(filter: impl Fn(&wgpu::Adapter) -> bool) -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut adapters = pollster::block_on(
        instance.enumerate_adapters(wgpu::Backends::VULKAN | wgpu::Backends::METAL),
    )
    .into_iter()
    .filter(filter)
    .collect::<Vec<_>>();
    adapters.sort_by_key(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu);

    let adapter = adapters.last().unwrap();

    adapter
        .request_device_with_video_support(&VideoDeviceDescriptor::default())
        .unwrap()
}

fn frame_data_diff(actual: &[u8], expected: &[u8]) -> f32 {
    if actual.len() != expected.len() {
        return f32::MAX;
    }

    let diff = actual.iter().zip(expected).fold(0.0, |total_err, (a, b)| {
        total_err + (*a as f32 - *b as f32).powf(2.0)
    });

    diff / actual.len() as f32
}
