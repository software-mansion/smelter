#[cfg(vulkan)]
fn main() {
    use std::{
        io::{Read, Write},
        num::NonZeroU32,
    };

    use gpu_video::{
        EncodedOutputChunk, InputFrame, RawFrameData, VideoInstance,
        parameters::{
            EncoderParametersH264, EncoderParametersH265, RateControl, VideoAdapterDescriptor,
            VideoDeviceDescriptor, VideoInstanceDescriptor, VideoParameters,
        },
    };

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

    // TODO: remake this example
    let (h264_chunk_sender, h264_chunk_receiver) =
        std::sync::mpsc::channel::<EncodedOutputChunk<Vec<u8>>>();
    let (h265_chunk_sender, h265_chunk_receiver) =
        std::sync::mpsc::channel::<EncodedOutputChunk<Vec<u8>>>();

    let h264_waiter_thread_handle = std::thread::spawn(move || {
        let mut output_file = std::fs::File::create("output.h264").unwrap();
        for chunk in h264_chunk_receiver.iter() {
            output_file.write_all(&chunk.data).unwrap();
        }
    });
    let h265_waiter_thread_handle = std::thread::spawn(move || {
        let mut output_file = std::fs::File::create("output.h265").unwrap();
        for chunk in h265_chunk_receiver.iter() {
            output_file.write_all(&chunk.data).unwrap();
        }
    });

    let on_h264_chunk = move |chunk| {
        h264_chunk_sender.send(chunk).unwrap();
    };
    let on_h265_chunk = move |chunk| {
        h265_chunk_sender.send(chunk).unwrap();
    };

    let mut encoder_h264 = video_device
        .create_bytes_encoder_h264(
            EncoderParametersH264 {
                input_parameters: VideoParameters {
                    width,
                    height,
                    target_framerate: 24.into(),
                },
                output_parameters: video_device
                    .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
                        average_bitrate: 1_000_000,
                        max_bitrate: 2_000_000,
                        virtual_buffer_size: std::time::Duration::from_secs(2),
                    })
                    .unwrap(),
            },
            on_h264_chunk,
        )
        .expect("create encoder");

    let mut encoder_h265 = video_device
        .create_bytes_encoder_h265(
            EncoderParametersH265 {
                input_parameters: VideoParameters {
                    width,
                    height,
                    target_framerate: 24.into(),
                },
                output_parameters: video_device
                    .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
                        average_bitrate: 1_000_000,
                        max_bitrate: 2_000_000,
                        virtual_buffer_size: std::time::Duration::from_secs(2),
                    })
                    .unwrap(),
            },
            on_h265_chunk,
        )
        .expect("create encoder");

    let mut frame = InputFrame {
        data: RawFrameData {
            frame: vec![0; width.get() as usize * height.get() as usize * 3 / 2],
            width: width.get(),
            height: height.get(),
        },
        pts: None,
    };

    while let Ok(()) = nv12.read_exact(&mut frame.data.frame) {
        encoder_h264.encode(&frame, false).expect("encode");
        encoder_h265.encode(&frame, false).expect("encode");
    }

    encoder_h264.flush().expect("flush");
    encoder_h265.flush().expect("flush");
    drop(encoder_h264);
    drop(encoder_h265);

    h264_waiter_thread_handle.join().unwrap();
    h265_waiter_thread_handle.join().unwrap();
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
