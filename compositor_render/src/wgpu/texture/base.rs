use crate::wgpu::WgpuCtx;

use super::utils::pad_to_256;

#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Texture {
    pub(super) const DEFAULT_BINDING_TYPE: wgpu::BindingType = wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    };

    pub fn new(
        device: &wgpu::Device,
        label: Option<&str>,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[format],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }

    pub fn copy_wgpu_texture(&self, ctx: &WgpuCtx) -> wgpu::Texture {
        let destination = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: self.texture.size(),
            mip_level_count: self.texture.mip_level_count(),
            sample_count: self.texture.sample_count(),
            dimension: self.texture.dimension(),
            format: self.texture.format(),
            usage: self.texture.usage(),
            view_formats: &[self.texture.format()],
        });
        copy_texture_to_texture(ctx, &self.texture, &destination);
        destination
    }

    pub fn fill_from_wgpu_texture(
        &self,
        ctx: &WgpuCtx,
        source: &wgpu::Texture,
    ) -> Result<(), TextureCopyError> {
        let expected = (
            self.texture.size(),
            self.texture.mip_level_count(),
            self.texture.sample_count(),
            self.texture.dimension(),
            self.texture.format(),
            self.texture.usage(),
        );
        let actual = (
            source.size(),
            source.mip_level_count(),
            source.sample_count(),
            source.dimension(),
            source.format(),
            source.usage(),
        );

        if expected != actual {
            return Err(TextureCopyError {
                expected: format!("{expected:?}"),
                actual: format!("{actual:?}"),
            });
        }
        copy_texture_to_texture(ctx, source, &self.texture);
        Ok(())
    }

    pub fn empty(device: &wgpu::Device) -> Self {
        Self::new(
            device,
            Some("empty texture"),
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureUsages::TEXTURE_BINDING,
        )
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }

    pub(super) fn upload_data(&self, queue: &wgpu::Queue, data: &[u8], bytes_per_pixel: u32) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: &self.texture,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.texture.width() * bytes_per_pixel),
                rows_per_image: Some(self.texture.height()),
            },
            self.texture.size(),
        );
    }

    /// Returns `None` for some depth formats
    pub(super) fn block_size(&self) -> Option<u32> {
        self.texture.format().block_copy_size(None)
    }

    pub(super) fn new_download_buffer(&self, ctx: &WgpuCtx) -> wgpu::Buffer {
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
    pub(super) fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer) {
        let size = self.size();
        let block_size = self.block_size().unwrap();

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: &self.texture,
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

#[derive(Debug, thiserror::Error)]
#[error("Passed invalid texture. Expected: {expected}, Actual: {actual}")]
pub struct TextureCopyError {
    expected: String,
    actual: String,
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
