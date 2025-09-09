use crate::wgpu::WgpuCtx;

use super::utils::pad_to_256;

pub(super) const DEFAULT_BINDING_TYPE: wgpu::BindingType = wgpu::BindingType::Texture {
    sample_type: wgpu::TextureSampleType::Float { filterable: true },
    view_dimension: wgpu::TextureViewDimension::D2,
    multisampled: false,
};

pub fn new_texture(
    device: &wgpu::Device,
    label: Option<&str>,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: &[wgpu::TextureFormat],
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

pub trait TextureExt {
    fn clone_texture(&self, ctx: &WgpuCtx, view_formats: &[wgpu::TextureFormat]) -> wgpu::Texture;

    fn upload_data(&self, queue: &wgpu::Queue, data: &[u8], bytes_per_pixel: u32);

    /// Returns `None` for some depth formats
    fn block_size(&self) -> Option<u32>;

    fn new_download_buffer(&self, ctx: &WgpuCtx) -> wgpu::Buffer;

    /// [`wgpu::Queue::submit`] has to be called afterwards
    fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer);
}

impl TextureExt for wgpu::Texture {
    fn clone_texture(&self, ctx: &WgpuCtx, view_formats: &[wgpu::TextureFormat]) -> wgpu::Texture {
        let destination = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: self.size(),
            mip_level_count: self.mip_level_count(),
            sample_count: self.sample_count(),
            dimension: self.dimension(),
            format: self.format(),
            usage: self.usage(),
            view_formats,
        });
        copy_texture_to_texture(ctx, self, &destination);
        destination
    }

    fn upload_data(&self, queue: &wgpu::Queue, data: &[u8], bytes_per_pixel: u32) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: self,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width() * bytes_per_pixel),
                rows_per_image: Some(self.height()),
            },
            self.size(),
        );
    }

    /// Returns `None` for some depth formats
    fn block_size(&self) -> Option<u32> {
        self.format().block_copy_size(None)
    }

    fn new_download_buffer(&self, ctx: &WgpuCtx) -> wgpu::Buffer {
        let size = self.size();
        let block_size = self.block_size().unwrap();

        ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture buffer"),
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            size: (pad_to_256(block_size * size.width) * size.height) as u64,
        })
    }

    /// [`wgpu::Queue::submit`] has to be called afterwards
    fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer) {
        let size = self.size();
        let block_size = self.block_size().unwrap();

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: self,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    bytes_per_row: Some(pad_to_256(block_size * size.width)),
                    rows_per_image: Some(size.height),
                    offset: 0,
                },
            },
            size,
        );
    }
}

fn copy_texture_to_texture(ctx: &WgpuCtx, source: &wgpu::Texture, destination: &wgpu::Texture) {
    let mut encoder = ctx.device.create_command_encoder(&Default::default());

    encoder.copy_texture_to_texture(
        source.as_image_copy(),
        destination.as_image_copy(),
        source.size(),
    );
    ctx.queue.submit(Some(encoder.finish()));
}
