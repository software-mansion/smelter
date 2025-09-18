use tracing::error;

use crate::wgpu::{texture::utils::pad_to_256, WgpuCtx};

pub struct ReinterpretToSrgb {
    buffer: wgpu::Buffer,
}

impl ReinterpretToSrgb {
    pub fn new(ctx: &WgpuCtx) -> Self {
        Self {
            buffer: ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("temporary buffer to reinterpret texture as srgb"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
        }
    }

    fn ensure_size(&mut self, ctx: &WgpuCtx, size: wgpu::Extent3d) {
        let expected_size = (pad_to_256(4 * size.width) * size.height) as u64;
        if self.buffer.size() == expected_size {
            return;
        }
        self.buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temporary buffer to reinterpret texture as srgb"),
            size: expected_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, source: &wgpu::Texture, dest: &wgpu::Texture) {
        let size = source.size();
        if dest.size() != source.size() {
            error!("Destination and source sizes does not match when reinterpreting to sRGB.");
            return;
        }

        self.ensure_size(ctx, size);

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy static image asset to texture"),
            });

        let buffer_desc = wgpu::TexelCopyBufferInfo {
            buffer: &self.buffer,
            layout: wgpu::TexelCopyBufferLayout {
                bytes_per_row: Some(pad_to_256(4 * size.width)),
                rows_per_image: Some(size.height),
                offset: 0,
            },
        };

        encoder.copy_texture_to_buffer(source.as_image_copy(), buffer_desc, size);
        encoder.copy_buffer_to_texture(buffer_desc, dest.as_image_copy(), size);

        ctx.queue.submit(Some(encoder.finish()));
    }
}
