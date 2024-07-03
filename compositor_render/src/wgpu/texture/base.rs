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

    pub fn copy_to_wgpu_texture(&self, ctx: &WgpuCtx) -> wgpu::Texture {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());

        let dst_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: self.texture.size(),
            mip_level_count: self.texture.mip_level_count(),
            sample_count: self.texture.sample_count(),
            dimension: self.texture.dimension(),
            format: self.texture.format(),
            usage: self.texture.usage(),
            view_formats: &[self.texture.format()],
        });
        encoder.copy_texture_to_texture(
            self.texture.as_image_copy(),
            dst_texture.as_image_copy(),
            self.texture.size(),
        );
        ctx.queue.submit(Some(encoder.finish()));
        dst_texture
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
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING,
        )
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }

    pub(super) fn upload_data(&self, queue: &wgpu::Queue, data: &[u8], bytes_per_pixel: u32) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: &self.texture,
            },
            data,
            wgpu::ImageDataLayout {
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
            size: (block_size * pad_to_256(size.width) * size.height) as u64,
        })
    }

    /// [`wgpu::Queue::submit`] has to be called afterwards
    pub(super) fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer) {
        let size = self.size();
        let block_size = self.block_size().unwrap();

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                texture: &self.texture,
            },
            wgpu::ImageCopyBuffer {
                buffer,
                layout: wgpu::ImageDataLayout {
                    bytes_per_row: Some(block_size * pad_to_256(size.width)),
                    rows_per_image: Some(size.height),
                    offset: 0,
                },
            },
            size,
        );
    }
}
