#[cfg(vulkan)]
fn main() {
    use vk_video::{
        EncodedInputChunk, OutputFrame, VulkanInstance,
        parameters::{DecoderParameters, VulkanAdapterDescriptor, VulkanDeviceDescriptor},
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        println!("usage: {} FILENAME", args[0]);
        return;
    }

    let h264_bytestream = std::fs::read(&args[1]).unwrap_or_else(|_| panic!("read {}", args[1]));

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_adapter = vulkan_instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&VulkanDeviceDescriptor::default())
        .unwrap();

    let mut decoder = vulkan_device
        .create_bytes_decoder(DecoderParameters::default())
        .unwrap();

    std::fs::create_dir_all("output_frames").unwrap();

    let mut frame_index = 0u32;

    for chunk in h264_bytestream.chunks(256) {
        let data = EncodedInputChunk {
            data: chunk,
            pts: None,
        };

        let frames = decoder.decode(data).unwrap();

        for OutputFrame { data, .. } in frames {
            save_nv12_as_png(&data.frame, data.width, data.height, frame_index);
            frame_index += 1;
        }
    }

    let remaining_frames = decoder.flush().unwrap();
    for OutputFrame { data, .. } in remaining_frames {
        save_nv12_as_png(&data.frame, data.width, data.height, frame_index);
        frame_index += 1;
    }

    println!("Saved {frame_index} frames to output_frames/");
}

#[cfg(vulkan)]
fn save_nv12_as_png(nv12: &[u8], width: u32, height: u32, index: u32) {
    if index < 60 || index > 70 {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let y_plane = &nv12[..w * h];
    let uv_plane = &nv12[w * h..];

    let mut rgb = vec![0u8; w * h * 3];

    for row in 0..h {
        for col in 0..w {
            let y = y_plane[row * w + col] as f32;
            let uv_idx = (row / 2) * w + (col & !1);
            let u = uv_plane[uv_idx] as f32 - 128.0;
            let v = uv_plane[uv_idx + 1] as f32 - 128.0;

            let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

            let pixel = (row * w + col) * 3;
            rgb[pixel] = r;
            rgb[pixel + 1] = g;
            rgb[pixel + 2] = b;
        }
    }

    let path = format!("output_frames/frame_{index:05}.png");
    image::save_buffer(&path, &rgb, width, height, image::ColorType::Rgb8).unwrap();
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
