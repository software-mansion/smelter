use std::{num::NonZeroU32, time::Duration};

use vk_video::parameters::{RateControl, Rational, VideoParameters};

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

    let transcoder = device
        .create_transcoder(&[device
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
            .unwrap()])
        .unwrap();
}

fn print_usage_and_exit(executable_name: &str) -> ! {
    println!("usage: {} INPUT OUT_WIDTH OUT_HEIGHT", executable_name);
    std::process::exit(0);
}
