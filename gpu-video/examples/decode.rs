#[cfg(vulkan)]
fn main() {
    use std::io::Write;

    use gpu_video::{
        EncodedInputChunk, OutputFrame, RawFrameData, VideoInstance,
        parameters::{
            DecoderParameters, VideoAdapterDescriptor, VideoDeviceDescriptor,
            VideoInstanceDescriptor,
        },
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

    let (frame_sender, frame_receiver) = std::sync::mpsc::channel::<OutputFrame<RawFrameData>>();

    let writer_thread_handle = std::thread::spawn(move || {
        let mut output_file = std::fs::File::create("output.nv12").unwrap();
        for frame in frame_receiver.iter() {
            output_file.write_all(&frame.data.frame).unwrap();
        }
    });

    let on_frame = move |output_frame| {
        frame_sender.send(output_frame).unwrap();
    };

    let mut decoder = video_device
        .create_bytes_decoder_h264(DecoderParameters::default(), on_frame)
        .unwrap();

    for chunk in h264_bytestream.chunks(256) {
        let data = EncodedInputChunk {
            data: chunk,
            pts: None,
        };

        decoder.decode(data).unwrap();
    }

    decoder.flush().unwrap();
    drop(decoder);

    writer_thread_handle.join().unwrap();
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
