#[cfg(vulkan)]
fn main() {
    use vk_video::{Frame, VulkanInstance};

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

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

    vulkan_device.create_converter(
        vk_video::H264Profile::High,
        1280,
        720,
        30,
        vk_video::RateControl::Vbr {
            average_bitrate: 500000,
            max_bitrate: 2000000,
        },
    ).unwrap();
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
