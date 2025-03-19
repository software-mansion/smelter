use crate::{wgpu::{texture::RgbaLinearTexture, WgpuCtx}, Resolution};

pub(super) struct RgbToSrgbConverter {
    texture: RgbaLinearTexture;
}

impl RgbToSrgbConverter {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self { texture: RgbaLinearTexture::new(ctx, resolution) }
    }

    pub fn convert(ctx: &WgpuCtx, destination: wgpu::TextureView){
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy static image asset to texture"),
            });
    
        let size = source.size();
        let target = target.ensure_size(
            ctx,
            Resolution {
                width: size.width as usize,
                height: size.height as usize,
            },
        );
    
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: source.texture(),
            },
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: target.rgba_texture().texture(),
            },
            size,
        );
    
        ctx.queue.submit(Some(encoder.finish()));
    } 
}
