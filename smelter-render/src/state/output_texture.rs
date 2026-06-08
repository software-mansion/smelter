use std::io::Write;

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use std::sync::Arc;

use bytes::BufMut;
use crossbeam_channel::bounded;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use gpu_video::{DmaBufError, DmaBufFrame, VideoResolution, export_nv12_dmabuf_texture};
use tracing::error;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use tracing::{info, warn};
use wgpu::{Buffer, BufferAsyncError};

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use crate::wgpu::texture::NV12Texture;
use crate::{
    OutputFrameFormat, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{
            PlanarYuvPendingDownload, PlanarYuvTextures, PlanarYuvVariant,
            utils::pad_to_256,
        },
    },
};

pub enum OutputTexture {
    PlanarYuvTextures(Box<PlanarYuvOutput>),
    Rgba8UnormWgpuTexture {
        resolution: Resolution,
    },
    Nv12WgpuTexture {
        resolution: Resolution,
    },
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    Nv12DmaBuf(Box<Nv12DmaBufOutput>),
}

#[derive(Debug, thiserror::Error)]
pub enum CreateOutputTextureError {
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    #[error("failed to create NV12 DMA-BUF output frame: {0}")]
    DmaBuf(#[from] DmaBufError),

    #[error("NV12 DMA-BUF output frame has a non-NV12 wgpu texture")]
    InvalidDmaBufTexture,
}

impl OutputTexture {
    pub fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
        format: OutputFrameFormat,
    ) -> Result<Self, CreateOutputTextureError> {
        match format {
            OutputFrameFormat::PlanarYuv420Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV420)),
            )),
            OutputFrameFormat::PlanarYuv422Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV422)),
            )),
            OutputFrameFormat::PlanarYuv444Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV444)),
            )),
            OutputFrameFormat::RgbaWgpuTexture => {
                Ok(Self::Rgba8UnormWgpuTexture { resolution })
            }
            OutputFrameFormat::Nv12WgpuTexture => {
                Ok(Self::Nv12WgpuTexture { resolution })
            }
            #[cfg(all(feature = "dmabuf", target_os = "linux"))]
            OutputFrameFormat::Nv12DmaBuf => {
                info!(?resolution, "creating zero-copy NV12 DMA-BUF output texture");
                Ok(Self::Nv12DmaBuf(Box::new(Nv12DmaBufOutput::new(ctx, resolution)?)))
            }
        }
    }
}

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
pub struct Nv12DmaBufOutput {
    device: Arc<wgpu::Device>,
    frames: Vec<PooledNv12DmaBufFrame>,
    next_index: usize,
    resolution: Resolution,
}

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
struct PooledNv12DmaBufFrame {
    dmabuf: Arc<DmaBufFrame>,
    texture: NV12Texture,
}

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
impl Nv12DmaBufOutput {
    const POOL_SIZE: usize = 16;

    fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
    ) -> Result<Self, CreateOutputTextureError> {
        let frames = (0..Self::POOL_SIZE)
            .map(|_| Self::new_frame(&ctx.device, resolution))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { device: Arc::clone(&ctx.device), frames, next_index: 0, resolution })
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn next_frame(
        &mut self,
    ) -> Result<(&NV12Texture, Arc<DmaBufFrame>), CreateOutputTextureError> {
        let index = self.next_available_frame_index()?;
        let frame = &self.frames[index];
        Ok((&frame.texture, Arc::clone(&frame.dmabuf)))
    }

    fn next_available_frame_index(&mut self) -> Result<usize, CreateOutputTextureError> {
        for _ in 0..self.frames.len() {
            let index = self.next_index;
            self.next_index = (self.next_index + 1) % self.frames.len();
            if Arc::strong_count(&self.frames[index].dmabuf) == 1 {
                return Ok(index);
            }
        }

        self.grow_pool()?;
        Ok(self.frames.len() - 1)
    }

    fn grow_pool(&mut self) -> Result<(), CreateOutputTextureError> {
        let frame = Self::new_frame(&self.device, self.resolution)?;
        self.frames.push(frame);
        warn!(
            pool_size = self.frames.len(),
            resolution = ?self.resolution,
            "grew zero-copy NV12 DMA-BUF output pool because every frame is still in flight"
        );
        Ok(())
    }

    fn new_frame(
        device: &wgpu::Device,
        resolution: Resolution,
    ) -> Result<PooledNv12DmaBufFrame, CreateOutputTextureError> {
        let dmabuf = export_nv12_dmabuf_texture(device, video_resolution(resolution))?;
        let texture = NV12Texture::from_wgpu_texture(dmabuf.texture_arc())
            .map_err(|_| CreateOutputTextureError::InvalidDmaBufTexture)?;
        Ok(PooledNv12DmaBufFrame { dmabuf, texture })
    }
}

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
fn video_resolution(resolution: Resolution) -> VideoResolution {
    VideoResolution { width: resolution.width as u32, height: resolution.height as u32 }
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
