#[cfg(vulkan)]
fn main() {
    use std::{
        fs::File,
        io::{Read, Write},
        num::NonZeroU32,
        time::Duration,
    };

    use vk_video::{
        EncodedInputChunk,
        parameters::{RateControl, VideoParameters},
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        print_usage_and_exit(&args[0]);
    }

    let input_file = &args[1];
    let Ok(output_width) = args[2].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };
    let Ok(output_height) = args[3].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };

    let instance = vk_video::VulkanInstance::new().unwrap();
    let adapter = instance.create_adapter(None).unwrap();
    let device = adapter
        .create_device(Default::default(), Default::default(), Default::default())
        .unwrap();

    let params = device
        .encoder_parameters_high_quality(
            VideoParameters {
                width: output_width,
                height: output_height,
                target_framerate: 30.into(),
            },
            RateControl::VariableBitrate {
                average_bitrate: 10_000_000,
                max_bitrate: 12_000_000,
                virtual_buffer_size: Duration::from_secs(2),
            },
        )
        .unwrap();

    let mut transcoder = device.create_transcoder(&[params]).unwrap();

    let mut input_file = File::open(input_file).unwrap();
    let mut output_file = File::create("output.h264").unwrap();

    let mut buffer = vec![0; 4096];
    while let Ok(n) = input_file.read(&mut buffer)
        && n > 0
    {
        let input = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };
        let output = transcoder.transcode(input).unwrap();

        for output in output {
            output_file.write_all(&output[0].data).unwrap();
        }
    }

    let flushed = transcoder.flush().unwrap();
    for output in flushed {
        output_file.write_all(&output[0].data).unwrap();
    }
}

#[cfg(vulkan)]
fn print_usage_and_exit(executable_name: &str) -> ! {
    eprintln!("usage: {executable_name} INPUT OUT_WIDTH OUT_HEIGHT");
    std::process::exit(1);
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
