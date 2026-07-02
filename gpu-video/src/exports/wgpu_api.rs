#[derive(thiserror::Error, Debug)]
pub enum WgpuInitError {
    #[error("Wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("Wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("Cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,
}
