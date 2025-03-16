use self::r8_fill_with_color::R8FillWithValue;

use super::WgpuCtx;

mod r8_fill_with_color;

#[derive(Debug)]
pub struct TextureUtils {
    pub r8_fill_with_value: R8FillWithValue,
}

impl TextureUtils {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            r8_fill_with_value: R8FillWithValue::new(device),
        }
    }

    pub fn fill_r8_with_value(&self, ctx: &WgpuCtx, dst: &wgpu::TextureView, value: f32) {
        self.r8_fill_with_value.fill(ctx, dst, value)
    }
}
