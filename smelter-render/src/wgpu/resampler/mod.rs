use super::format::TextureFormat;

mod bilinear;
mod lanczos3;

pub use bilinear::BilinearResampler;
pub use lanczos3::{Lanczos3Resampler, Lanczos3TextureCache};

#[derive(Debug)]
pub struct Resamplers {
    pub bilinear_linear: BilinearResampler,
    pub bilinear_srgb: BilinearResampler,
    pub lanczos3_linear: Lanczos3Resampler,
    pub lanczos3_srgb: Lanczos3Resampler,
}

impl Resamplers {
    pub fn new(device: &wgpu::Device, format: &TextureFormat) -> Self {
        let layout = &format.single_texture_layout;
        Self {
            bilinear_linear: BilinearResampler::new(
                device,
                layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            bilinear_srgb: BilinearResampler::new(
                device,
                layout,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ),
            lanczos3_linear: Lanczos3Resampler::new(
                device,
                layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            lanczos3_srgb: Lanczos3Resampler::new(
                device,
                layout,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ),
        }
    }
}
