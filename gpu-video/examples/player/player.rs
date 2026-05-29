use std::{path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use gpu_video::{VideoAdapterExt, parameters::VideoDeviceDescriptor};
use winit::{event_loop::EventLoop, window::WindowBuilder};

mod decoder;
mod renderer;

const FRAMES_BUFFER_LEN: usize = 3;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Args {
    /// an .h264 file to play
    filename: PathBuf,

    /// framerate to play the video at
    framerate: u64,
}

struct FrameWithPts {
    frame: wgpu::Texture,
    /// Presentation timestamp
    pts: Duration,
}

pub async fn run() {
    let args = Args::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let file = std::fs::File::open(&args.filename).expect("open file");

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .enumerate_adapters(wgpu::Backends::VULKAN)
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

    let surface = instance.create_surface(&window).unwrap();

    let (tx, rx) = mpsc::sync_channel(FRAMES_BUFFER_LEN);
    let device_clone = device.clone();

    std::thread::spawn(move || {
        decoder::run_decoder(tx, args.framerate, device_clone, file);
    });

    renderer::run_renderer(event_loop, &window, surface, adapter, device, queue, rx);
}
