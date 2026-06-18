#[cfg(vulkan)]
fn main() {
    use gpu_video::{
        VideoInstance,
        parameters::{VideoAdapterDescriptor, VideoDeviceDescriptor, VideoInstanceDescriptor},
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

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

    std::hint::black_box(video_device);
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
