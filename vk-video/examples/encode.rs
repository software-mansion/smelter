#[cfg(vulkan)]
fn main() {
    use std::{
        io::{Read, Write},
        num::NonZeroU32,
    };

    use vk_video::{Frame, RateControl, Rational, RawFrameData, VideoParameters, VulkanInstance};

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        println!("usage: {} FILENAME WIDTH HEIGHT", args[0]);
        return;
    }

    let width = args[2].parse::<NonZeroU32>().expect("parse video width");
    let height = args[3].parse::<NonZeroU32>().expect("parse video height");
    let mut nv12 =
        std::fs::File::open(&args[1]).unwrap_or_else(|e| panic!("open {}: {}", args[1], e));

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_device = vulkan_instance
        .create_device(
            wgpu::Features::empty(),
            wgpu::Limits {
                max_push_constant_size: 128,
                ..Default::default()
            },
            None,
        )
        .unwrap();

    let mut encoder = vulkan_device
        .create_bytes_encoder(vulkan_device.encoder_parameters_high_quality(
            VideoParameters {
                width,
                height,
                target_framerate: Rational {
                    numerator: 24,
                    denominator: NonZeroU32::new(1).unwrap(),
                },
            },
            RateControl::Vbr {
                average_bitrate: 1_000_000,
                max_bitrate: 4_000_000,
            },
        ))
        .expect("create encoder");

    let mut output_file = std::fs::File::create("output.h264").unwrap();

    let mut frame = Frame {
        data: RawFrameData {
            frame: vec![0; width.get() as usize * height.get() as usize * 3 / 2],
            width: width.get(),
            height: height.get(),
        },
        pts: None,
    };

    while let Ok(()) = nv12.read_exact(&mut frame.data.frame) {
        let h264 = encoder.encode(&frame, false).expect("encode");
        output_file.write_all(&h264.data).expect("write");
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
