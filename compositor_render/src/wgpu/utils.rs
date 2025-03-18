use self::r8_fill_with_color::R8FillWithValue;
use add_premultiplied_alpha::AddPremultipliedAlpha;
use remove_premultiplied_alpha::RemovePremultipliedAlpha;

use super::{format::TextureFormat, WgpuCtx};

mod add_premultiplied_alpha;
mod r8_fill_with_color;
mod remove_premultiplied_alpha;

#[derive(Debug)]
pub struct TextureUtils {
    pub r8_fill_with_value: R8FillWithValue,
    pub linear_rgba_remove_premult_alpha: RemovePremultipliedAlpha,
    pub srgb_rgba_remove_premult_alpha: RemovePremultipliedAlpha,
    pub linear_rgba_add_premult_alpha: AddPremultipliedAlpha,
    pub srgb_rgba_add_premult_alpha: AddPremultipliedAlpha,
}

impl TextureUtils {
    pub fn new(device: &wgpu::Device, format: &TextureFormat) -> Self {
        Self {
            r8_fill_with_value: R8FillWithValue::new(device),
            linear_rgba_remove_premult_alpha: RemovePremultipliedAlpha::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            srgb_rgba_remove_premult_alpha: RemovePremultipliedAlpha::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ),
            linear_rgba_add_premult_alpha: AddPremultipliedAlpha::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            srgb_rgba_add_premult_alpha: AddPremultipliedAlpha::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ),
        }
    }

    pub fn fill_r8_with_value(&self, ctx: &WgpuCtx, dst: &wgpu::TextureView, value: f32) {
        self.r8_fill_with_value.fill(ctx, dst, value)
    }
}
