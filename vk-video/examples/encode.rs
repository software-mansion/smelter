#[cfg(vulkan)]
fn main() {
    use std::io::{Read, Write};

    use vk_video::{Frame, RawFrame, VulkanInstance};

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        println!("usage: {} FILENAME WIDTH HEIGHT", args[0]);
        return;
    }

    let width = args[2].parse::<u32>().expect("parse video width");
    let height = args[3].parse::<u32>().expect("parse video height");
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
            &mut None,
        )
        .unwrap();

    let mut encoder = vulkan_device
        .crate_encoder(
            vk_video::H264Profile::High,
            width,
            height,
            30,
            vk_video::RateControl::Vbr { average_bitrate: 500000, max_bitrate: 2000000 },
            // vk_video::RateControl::Disabled,
        )
        .expect("create encoder");

    let mut output_file = std::fs::File::create("output.h264").unwrap();

    let mut frame = Frame {
        frame: RawFrame {
            data: vec![0; width as usize * height as usize * 3 / 2],
            width,
            height,
        },
        pts: None,
    };

    while let Ok(()) = nv12.read_exact(&mut frame.frame.data) {
        let h264 = encoder.encode_bytes(&frame, false).expect("encode");
        output_file.write_all(&h264).expect("write");
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
