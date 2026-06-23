use std::{cell::RefCell, io::Write, sync::Arc};

use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;
use wgpu::{Buffer, BufferAsyncError};

use crate::{
    OutputFrameFormat, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{
            NV12Texture, PlanarYuvPendingDownload, PlanarYuvTextures, PlanarYuvVariant,
            utils::pad_to_256,
        },
    },
};

pub enum OutputTexture {
    PlanarYuvTextures(Box<PlanarYuvOutput>),
    Rgba8UnormWgpuTexture(RgbaWgpuOutput),
    Nv12WgpuTexture(Nv12WgpuOutput),
}

impl OutputTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution, format: OutputFrameFormat) -> Self {
        match format {
            OutputFrameFormat::PlanarYuv420Bytes => Self::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV420),
            )),
            OutputFrameFormat::PlanarYuv422Bytes => Self::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV422),
            )),
            OutputFrameFormat::PlanarYuv444Bytes => Self::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV444),
            )),
            OutputFrameFormat::RgbaWgpuTexture => {
                Self::Rgba8UnormWgpuTexture(RgbaWgpuOutput::new(resolution))
            }
            OutputFrameFormat::Nv12WgpuTexture => {
                Self::Nv12WgpuTexture(Nv12WgpuOutput::new(resolution))
            }
        }
    }
}

pub struct RgbaWgpuOutput {
    resolution: Resolution,
    textures: RefCell<Vec<Arc<wgpu::Texture>>>,
}

impl RgbaWgpuOutput {
    fn new(resolution: Resolution) -> Self {
        Self { resolution, textures: RefCell::new(Vec::new()) }
    }

    pub fn copy_from(
        &self,
        ctx: &WgpuCtx,
        source: &wgpu::Texture,
        view_formats: &[wgpu::TextureFormat],
    ) -> Arc<wgpu::Texture> {
        let texture = {
            let mut textures = self.textures.borrow_mut();
            if let Some(texture) =
                textures.iter().find(|texture| Arc::strong_count(texture) == 1)
            {
                Arc::clone(texture)
            } else {
                let texture =
                    Arc::new(ctx.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("RGBA output frame"),
                        size: source.size(),
                        mip_level_count: source.mip_level_count(),
                        sample_count: source.sample_count(),
                        dimension: source.dimension(),
                        format: source.format(),
                        usage: source.usage(),
                        view_formats,
                    }));
                textures.push(Arc::clone(&texture));
                texture
            }
        };

        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            texture.as_image_copy(),
            source.size(),
        );
        ctx.queue.submit(Some(encoder.finish()));

        texture
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }
}

pub struct Nv12WgpuOutput {
    resolution: Resolution,
    textures: RefCell<Vec<NV12Texture>>,
}

impl Nv12WgpuOutput {
    fn new(resolution: Resolution) -> Self {
        Self { resolution, textures: RefCell::new(Vec::new()) }
    }

    pub fn convert_from_with_encoder(
        &self,
        ctx: &WgpuCtx,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::BindGroup,
    ) -> Arc<wgpu::Texture> {
        let mut textures = self.textures.borrow_mut();
        let texture_index = match textures.iter().position(NV12Texture::is_unused) {
            Some(index) => index,
            None => {
                textures.push(NV12Texture::new(ctx, self.resolution));
                textures.len() - 1
            }
        };
        let texture = &textures[texture_index];
        ctx.format.rgba_to_nv12.encode_convert(ctx, encoder, source, texture);
        texture.texture_arc()
    }

    pub fn fill_with_color(
        &self,
        ctx: &WgpuCtx,
        color: crate::scene::RGBColor,
    ) -> Arc<wgpu::Texture> {
        let mut textures = self.textures.borrow_mut();
        let texture_index = match textures.iter().position(NV12Texture::is_unused) {
            Some(index) => index,
            None => {
                textures.push(NV12Texture::new(ctx, self.resolution));
                textures.len() - 1
            }
        };
        let texture = &textures[texture_index];
        texture.fill_with_color(ctx, color);
        texture.texture_arc()
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }
}

pub struct PlanarYuvOutput {
    textures: PlanarYuvTextures,
    buffers: [wgpu::Buffer; 3],
    resolution: Resolution,
}

impl PlanarYuvOutput {
    pub fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
        pixel_format: PlanarYuvVariant,
    ) -> Self {
        let textures = PlanarYuvTextures::new(ctx, resolution, pixel_format);
        let buffers = textures.new_download_buffers(ctx);

        Self { textures, buffers, resolution }
    }

    pub fn yuv_textures(&self) -> &PlanarYuvTextures {
        &self.textures
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn start_download<'a>(
        &'a self,
        ctx: &WgpuCtx,
    ) -> PlanarYuvPendingDownload<
        'a,
        impl FnOnce() -> Result<bytes::Bytes, BufferAsyncError> + 'a,
        BufferAsyncError,
    > {
        self.textures.copy_to_buffers(ctx, &self.buffers);

        PlanarYuvPendingDownload::new(
            self.download_buffer(self.textures.plane_texture(0).size(), &self.buffers[0]),
            self.download_buffer(self.textures.plane_texture(1).size(), &self.buffers[1]),
            self.download_buffer(self.textures.plane_texture(2).size(), &self.buffers[2]),
        )
    }

    fn download_buffer<'a>(
        &'a self,
        size: wgpu::Extent3d,
        source: &'a Buffer,
    ) -> impl FnOnce() -> Result<bytes::Bytes, BufferAsyncError> + 'a {
        let buffer = bytes::BytesMut::with_capacity((size.width * size.height) as usize);
        let (s, r) = bounded(1);
        source.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            if let Err(err) = s.send(result) {
                error!("channel send error: {err}")
            }
        });

        move || {
            r.recv().unwrap()?;
            let mut buffer = buffer.writer();
            {
                let range = source.slice(..).get_mapped_range().unwrap();
                let chunks = range.chunks(pad_to_256(size.width) as usize);
                for chunk in chunks {
                    buffer.write_all(&chunk[..size.width as usize]).unwrap();
                }
            };
            source.unmap();
            Ok(buffer.into_inner().into())
        }
    }
}
