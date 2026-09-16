#[cfg(vulkan)]
fn main() {
    use std::{
        fs::File,
        io::{Read, Write},
        num::NonZeroU32,
        sync::mpsc::Sender,
        thread::JoinHandle,
        time::Duration,
    };

    use gpu_video::{
        EncodedInputChunk, EncodedOutputChunk, VideoInstance,
        parameters::{
            AnyEncoderParameters, RateControl, ScalingAlgorithm, TranscoderOutputParameters,
            TranscoderParameters, VideoAdapterDescriptor, VideoDeviceDescriptor,
            VideoInstanceDescriptor,
        },
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 || args.len() > 5 {
        print_usage_and_exit(&args[0]);
    }

    let input_file = &args[1];
    let Ok(output_width) = args[2].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };
    let Ok(output_height) = args[3].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };

    let scaling_algorithm = if args.len() == 5 {
        match args[4].as_str() {
            "nearest" => ScalingAlgorithm::NearestNeighbor,
            "bilinear" => ScalingAlgorithm::Bilinear,
            "lanczos3" => ScalingAlgorithm::Lanczos3,
            _ => print_usage_and_exit(&args[0]),
        }
    } else {
        ScalingAlgorithm::default()
    };

    let video_instance = VideoInstance::new(&VideoInstanceDescriptor {
        enable_validations: true,
        ..Default::default()
    })
    .unwrap();
    let video_adapter = video_instance
        .create_adapter(&VideoAdapterDescriptor::default())
        .unwrap();
    let video_device = video_adapter
        .create_device(&VideoDeviceDescriptor::default())
        .unwrap();

    let average_bitrate = 1_000_000;
    let max_bitrate = 1_200_000;

    let params_h264 = video_device
        .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
            average_bitrate,
            max_bitrate,
            virtual_buffer_size: Duration::from_secs(2),
        })
        .unwrap();

    let params_h265 = video_device
        .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
            average_bitrate,
            max_bitrate,
            virtual_buffer_size: Duration::from_secs(2),
        })
        .unwrap();

    let (thread_handles, chunk_senders): (Vec<_>, Vec<_>) = [
        spawn_writer_thread("output.h264"),
        spawn_writer_thread("output.h265"),
    ]
    .into_iter()
    .unzip();

    let mut transcoder = video_device
        .create_transcoder(
            TranscoderParameters {
                input_framerate: 30.into(),
                output_parameters: vec![
                    TranscoderOutputParameters {
                        output_width,
                        output_height,
                        encoder_parameters: AnyEncoderParameters::H264(params_h264),
                        scaling_algorithm,
                    },
                    TranscoderOutputParameters {
                        output_width,
                        output_height,
                        encoder_parameters: AnyEncoderParameters::H265(params_h265),
                        scaling_algorithm,
                    },
                ],
                max_in_flight_submissions: Some(3),
            },
            move |transcoded| {
                chunk_senders[transcoded.output_index]
                    .send(transcoded.chunk)
                    .unwrap();
            },
        )
        .unwrap();

    let mut input_file = File::open(input_file).unwrap();

    let mut buffer = vec![0; 4096];
    while let Ok(n) = input_file.read(&mut buffer)
        && n > 0
    {
        let input = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };
        transcoder.transcode(input).unwrap();
    }

    transcoder.flush().unwrap();

    drop(transcoder);
    for handle in thread_handles {
        handle.join().unwrap();
    }
}

#[cfg(vulkan)]
fn print_usage_and_exit(executable_name: &str) -> ! {
    eprintln!("usage: {executable_name} INPUT OUT_WIDTH OUT_HEIGHT [nearest|bilinear|lanczos3]");
    std::process::exit(1);
}

#[cfg(vulkan)]
fn spawn_writer_thread(
    file_name: &'static str,
) -> (
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Sender<gpu_video::EncodedOutputChunk<Vec<u8>>>,
) {
    use std::io::Write;

    let (chunk_sender, chunk_receiver) =
        std::sync::mpsc::channel::<gpu_video::EncodedOutputChunk<Vec<u8>>>();
    let writer_thread_handle = std::thread::spawn(move || {
        let mut output_file = std::fs::File::create(file_name).unwrap();
        for chunk in chunk_receiver.iter() {
            output_file.write_all(&chunk.data).unwrap();
        }
    });
    (writer_thread_handle, chunk_sender)
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
