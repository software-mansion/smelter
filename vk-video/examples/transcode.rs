use std::{
    fs::File,
    io::{Read, Write},
    num::NonZeroU32,
    time::Duration,
};

use vk_video::{
    EncodedInputChunk,
    parameters::{RateControl, Rational, VideoParameters},
};

fn main() {
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
                target_framerate: Rational {
                    numerator: 30,
                    denominator: NonZeroU32::new(1).unwrap(),
                },
            },
            RateControl::VariableBitrate {
                average_bitrate: 10_000_000,
                max_bitrate: 12_000_000,
                virtual_buffer_size: Duration::from_secs(2),
            },
        )
        .unwrap();

    let mut transcoder = device.create_transcoder(&[params, params, params]).unwrap();

    let mut input_file = File::open(input_file).unwrap();
    let mut output_file = File::create("output.h264").unwrap();
    let mut output_file2 = File::create("output2.h264").unwrap();
    let mut output_file3 = File::create("output3.h264").unwrap();

    let mut files = [&mut output_file, &mut output_file2, &mut output_file3];
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
            for (output, file) in output.iter().zip(files.iter_mut()) {
                file.write_all(&output.data).unwrap();
            }
            // output_file.write_all(&output[0].data).unwrap();
            // output_file2.write_all(&output[1].data).unwrap();
        }
    }

    let flushed = transcoder.flush().unwrap();
    for output in flushed {
        for (output, file) in output.iter().zip(files.iter_mut()) {
            file.write_all(&output.data).unwrap();
        }
        // output_file.write_all(&output[0].data).unwrap();
        // output_file2.write_all(&output[1].data).unwrap();
    }
}

fn print_usage_and_exit(executable_name: &str) -> ! {
    println!("usage: {} INPUT OUT_WIDTH OUT_HEIGHT", executable_name);
    std::process::exit(0);
}
