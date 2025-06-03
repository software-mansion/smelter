use core::panic;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use compositor_pipeline::{
    pipeline::{
        encoder::{
            ffmpeg_h264::{self, EncoderPreset},
            OutputPixelFormat, VideoEncoderOptions,
        },
        input::RawDataInputOptions,
        output::{
            rtp::{RtpConnectionOptions, RtpSenderOptions},
            OutputOptions,
        },
        rtp::RequestedPort,
        GraphicsContext, GraphicsContextOptions, Options, Pipeline, PipelineOutputEndCondition,
        RegisterOutputOptions,
    },
    queue::{PipelineEvent, QueueInputOptions},
};
use compositor_render::{
    error::ErrorStack,
    scene::{Component, InputStreamComponent},
    Frame, FrameData, InputId, OutputId, Resolution,
};
use integration_tests::{gstreamer::start_gst_receive_tcp_h264, test_input::TestInput};
use smelter::{
    config::read_config,
    logger::{self},
};
use tokio::runtime::Runtime;

const VIDEO_OUTPUT_PORT: u16 = 8002;

// Start simple pipeline with input that sends PCM audio and wgpu::Textures via Rust channel.
fn main() {
    ffmpeg_next::format::network::init();
    let config = read_config();
    logger::init_logger(config.logger.clone());
    let ctx = GraphicsContext::new(GraphicsContextOptions {
        features: wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        ..Default::default()
    })
    .unwrap();

    let (wgpu_device, wgpu_queue) = (ctx.device.clone(), ctx.queue.clone());
    // no chromium support, so we can ignore _event_loop
    let (pipeline, _event_loop) = Pipeline::new(Options {
        wgpu_ctx: Some(ctx),
        tokio_rt: Some(Arc::new(Runtime::new().unwrap())),
        ..(&config).into()
    })
    .unwrap_or_else(|err| {
        panic!(
            "Failed to start compositor.\n{}",
            ErrorStack::new(&err).into_string()
        )
    });
    let pipeline = Arc::new(Mutex::new(pipeline));
    let output_id = OutputId("output_1".into());
    let input_id = InputId("input_id".into());

    let output_options = RegisterOutputOptions {
        output_options: OutputOptions::Rtp(RtpSenderOptions {
            connection_options: RtpConnectionOptions::TcpServer {
                port: RequestedPort::Exact(VIDEO_OUTPUT_PORT),
            },
            video: Some(VideoEncoderOptions::H264(ffmpeg_h264::Options {
                preset: EncoderPreset::Ultrafast,
                resolution: Resolution {
                    width: 1280,
                    height: 720,
                },
                pixel_format: OutputPixelFormat::YUV420P,
                raw_options: vec![],
            })),
            audio: None,
        }),
        video: Some(compositor_pipeline::pipeline::OutputVideoOptions {
            initial: Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_id.clone(),
            }),
            end_condition: PipelineOutputEndCondition::Never,
        }),
        audio: None, // TODO: add audio example
    };

    let sender = Pipeline::register_raw_data_input(
        &pipeline,
        input_id.clone(),
        RawDataInputOptions {
            video: true,
            audio: false,
        },
        QueueInputOptions {
            required: true,
            offset: Some(Duration::ZERO),
            buffer_duration: None,
        },
    )
    .unwrap();

    Pipeline::register_output(&pipeline, output_id.clone(), output_options).unwrap();

    let frames = generate_frames(&wgpu_device, &wgpu_queue);

    start_gst_receive_tcp_h264("127.0.0.1", VIDEO_OUTPUT_PORT, false).unwrap();

    Pipeline::start(&pipeline);

    let video_sender = sender.video.unwrap();
    for frame in frames {
        video_sender.send(PipelineEvent::Data(frame)).unwrap();
    }
    thread::sleep(Duration::from_millis(30000));
}

fn generate_frames(device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<Frame> {
    let texture_a = create_texture(0, device, queue);
    let texture_b = create_texture(1, device, queue);
    let texture_c = create_texture(2, device, queue);
    let resolution = Resolution {
        width: 640,
        height: 360,
    };
    let mut frames = vec![];

    for i in 0..200 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_a.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    for i in 200..400 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_b.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    for i in 400..600 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_c.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    for i in 600..800 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_a.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    for i in 800..1000 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_b.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    for i in 1000..1200 {
        frames.push(Frame {
            data: FrameData::Rgba8UnormWgpuTexture(texture_c.clone()),
            resolution,
            pts: Duration::from_millis(i * 20),
        })
    }

    frames
}

fn create_texture(index: usize, device: &wgpu::Device, queue: &wgpu::Queue) -> Arc<wgpu::Texture> {
    let input = TestInput::new(index);
    let size = wgpu::Extent3d {
        width: input.resolution.width as u32,
        height: input.resolution.height as u32,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            texture: &texture,
        },
        &input.data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * 4),
            rows_per_image: Some(texture.height()),
        },
        size,
    );
    queue.submit([]);
    texture.into()
}
