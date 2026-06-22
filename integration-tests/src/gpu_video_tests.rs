use std::{path::PathBuf, sync::OnceLock};

use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};

mod h264_decode_tests;
mod harness;

// TODO: Encoder tests
// TODO: Parser tests?
// TODO: Tests on windows
// TODO: if test fails, dump it to file so that it's viewable

struct Nv12Frame {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

struct TestCase<Options> {
    /// Path relative to the gpu-video dumps directory in the snapshots submodule.
    pub dump_file_path: PathBuf,
    pub options: Options,
    /// Maximum allowed error between the gpu-video output and the ffmpeg reference output.
    pub allowed_error: f32,
}

fn video_device() -> &'static (wgpu::Device, wgpu::Queue) {
    static DEVICE: OnceLock<(wgpu::Device, wgpu::Queue)> = OnceLock::new();

    DEVICE.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
            .into_iter()
            .filter(|a| {
                a.video_adapter_info()
                    .is_some_and(|info| info.supports_decoding)
            })
            .collect::<Vec<_>>();
        adapters.sort_by_key(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu);

        let adapter = adapters.last().unwrap();

        adapter
            .request_device_with_video_support(&VideoDeviceDescriptor::default())
            .unwrap()
    })
}
