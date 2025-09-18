use crate::{scene::RGBAColor, RenderingMode};

use self::r8_fill_with_color::R8FillWithValue;
use add_premultiplied_alpha::PremultiplyAlphaPipeline;
use remove_premultiplied_alpha::RemovePremultipliedAlphaPipeline;

use super::{format::TextureFormat, WgpuCtx};

mod add_premultiplied_alpha;
mod r8_fill_with_color;
mod reinterpret_input_to_srgb;
mod remove_premultiplied_alpha;

pub use reinterpret_input_to_srgb::ReinterpretToSrgb;

#[derive(Debug)]
pub struct TextureUtils {
    pub r8_fill_with_value: R8FillWithValue,
    pub linear_rgba_remove_premult_alpha: RemovePremultipliedAlphaPipeline,
    pub srgb_rgba_add_premult_alpha: PremultiplyAlphaPipeline,
    pub linear_rgba_add_premult_alpha: PremultiplyAlphaPipeline,
}

impl TextureUtils {
    pub fn new(device: &wgpu::Device, format: &TextureFormat) -> Self {
        Self {
            r8_fill_with_value: R8FillWithValue::new(device),
            linear_rgba_remove_premult_alpha: RemovePremultipliedAlphaPipeline::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            srgb_rgba_add_premult_alpha: PremultiplyAlphaPipeline::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ),
            linear_rgba_add_premult_alpha: PremultiplyAlphaPipeline::new(
                device,
                &format.single_texture_layout,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        }
    }
}

pub fn convert_to_shader_color(ctx: &WgpuCtx, color: &RGBAColor) -> [f64; 4] {
    match ctx.mode {
        RenderingMode::GpuOptimized | RenderingMode::WebGl => {
            let a = color.3 as f64 / 255.0;
            [
                a * srgb_to_linear(color.0),
                a * srgb_to_linear(color.1),
                a * srgb_to_linear(color.2),
                a,
            ]
        }
        RenderingMode::CpuOptimized => {
            let a = color.3 as f64 / 255.0;
            [
                a * color.0 as f64 / 255.0,
                a * color.1 as f64 / 255.0,
                a * color.2 as f64 / 255.0,
                a,
            ]
        }
    }
}

fn srgb_to_linear(color: u8) -> f64 {
    let color = color as f64 / 255.0;
    if color < 0.04045 {
        color / 12.92
    } else {
        f64::powf((color + 0.055) / 1.055, 2.4)
    }
}
