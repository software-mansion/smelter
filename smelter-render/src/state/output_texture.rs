use std::{any::Any, cell::RefCell, io::Write, sync::Arc};

use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;
use wgpu::{Buffer, BufferAsyncError};

use crate::{
    ExternalNv12FramePool, OutputFrameFormat, Resolution,
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
    ExternalNv12WgpuTexture(ExternalNv12Output),
}

impl OutputTexture {
    pub fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
        format: OutputFrameFormat,
        external_nv12_pool: Option<Arc<dyn ExternalNv12FramePool>>,
    ) -> Self {
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
            OutputFrameFormat::Nv12WgpuTexture => match external_nv12_pool {
                Some(pool) => {
                    Self::ExternalNv12WgpuTexture(ExternalNv12Output::new(resolution, pool))
                }
                None => Self::Nv12WgpuTexture(Nv12WgpuOutput::new(resolution)),
            },
        }
    }
}

/// Zero-copy NV12 output: the compositor renders directly into encoder-owned
/// dma-buf textures acquired from an [`ExternalNv12FramePool`]. No intermediate
/// texture and no per-frame copy.
pub struct ExternalNv12Output {
    resolution: Resolution,
    pool: Arc<dyn ExternalNv12FramePool>,
}

impl ExternalNv12Output {
    fn new(resolution: Resolution, pool: Arc<dyn ExternalNv12FramePool>) -> Self {
        Self { resolution, pool }
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// The pool this output renders into, so the batch driver can call
    /// [`ExternalNv12FramePool::finish_write`] for staged tokens after the shared
    /// submit.
    pub fn pool(&self) -> Arc<dyn ExternalNv12FramePool> {
        Arc::clone(&self.pool)
    }

    /// Acquire a free dma-buf slot, record the composited NV12 convert (visible
    /// region + coded padding) into the caller's shared `encoder`, and stage the
    /// dma-buf write fence (no submit). Returns the slot's texture plus the stage
    /// token to finish after the single batched submit. `None` if the bounded
    /// pool is momentarily exhausted (the frame is dropped for this tick —
    /// bounded, never blocks).
    pub fn render_into_pool(
        &self,
        ctx: &WgpuCtx,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::BindGroup,
    ) -> Option<(Arc<wgpu::Texture>, Box<dyn Any + Send>)> {
        let slot = self.pool.acquire()?;
        let nv12 = match NV12Texture::from_wgpu_texture(Arc::clone(&slot.texture)) {
            Ok(texture) => texture,
            Err(err) => {
                error!("External NV12 pool slot texture is invalid: {err}");
                return None;
            }
        };
        ctx.format.rgba_to_nv12.encode_convert_external(
            ctx,
            encoder,
            source,
            &nv12,
            self.pool.visible_resolution(),
            self.pool.padding_luma(),
            self.pool.padding_chroma(),
        );
        match self.pool.stage_write(slot.index) {
            Ok(token) => Some((slot.texture, token)),
            Err(err) => {
                error!("External NV12 pool write stage failed: {err}");
                None
            }
        }
    }

    /// Empty-scene fallback: record a clear to limited-range black (luma 16 /
    /// chroma 128, the padding values) into the shared `encoder` and stage the
    /// write fence. Returns the slot texture plus the stage token.
    pub fn fill_black(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<(Arc<wgpu::Texture>, Box<dyn Any + Send>)> {
        let slot = self.pool.acquire()?;
        let nv12 = match NV12Texture::from_wgpu_texture(Arc::clone(&slot.texture)) {
            Ok(texture) => texture,
            Err(err) => {
                error!("External NV12 pool slot texture is invalid: {err}");
                return None;
            }
        };
        let (y_view, uv_view) = nv12.views();
        for (view, value) in
            [(y_view, self.pool.padding_luma()), (uv_view, self.pool.padding_chroma())]
        {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("external nv12 clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: value,
                            g: value,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        match self.pool.stage_write(slot.index) {
            Ok(token) => Some((slot.texture, token)),
            Err(err) => {
                error!("External NV12 pool clear stage failed: {err}");
                None
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

    pub fn convert_lanczos_vertical_from_with_encoder(
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
        ctx.format
            .rgba_to_nv12
            .encode_lanczos_vertical_convert(ctx, encoder, source, texture);
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
