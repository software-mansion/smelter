use crate::{
    Resolution,
    wgpu::{WgpuCtx, texture::RgbaLinearTexture, utils::ReinterpretToSrgb},
};

pub(super) struct RgbToSrgbConverter {
    pub texture: RgbaLinearTexture,
    pub reinterpeter: ReinterpretToSrgb,
}

impl RgbToSrgbConverter {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self {
            texture: RgbaLinearTexture::new(ctx, resolution),
            reinterpeter: ReinterpretToSrgb::new(ctx),
        }
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, dest: &wgpu::Texture) {
        self.reinterpeter.convert(ctx, self.texture.texture(), dest);
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }
}
