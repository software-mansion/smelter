use std::io::Write;

use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;
use wgpu::{Buffer, BufferAsyncError};

use crate::{
    wgpu::{
        texture::{utils::pad_to_256, PlanarYuvPendingDownload, PlanarYuvTextures},
        WgpuCtx,
    },
    OutputFrameFormat, Resolution,
};

pub enum OutputTexture {
    PlanarYuv420Textures(PlanarYuvOutput),
    Rgba8UnormWgpuTexture { resolution: Resolution },
}

impl OutputTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution, format: OutputFrameFormat) -> Self {
        match format {
            OutputFrameFormat::PlanarYuv420Bytes => {
                Self::PlanarYuv420Textures(PlanarYuvOutput::new(ctx, resolution))
            }
            OutputFrameFormat::RgbaWgpuTexture => Self::Rgba8UnormWgpuTexture { resolution },
        }
    }
}

pub struct PlanarYuvOutput {
    textures: PlanarYuvTextures,
    buffers: [wgpu::Buffer; 3],
    resolution: Resolution,
}

impl PlanarYuvOutput {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let textures = PlanarYuvTextures::new(ctx, resolution);
        let buffers = textures.new_download_buffers(ctx);

        Self {
            textures,
            buffers,
            resolution,
        }
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
        source
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if let Err(err) = s.send(result) {
                    error!("channel send error: {err}")
                }
            });

        move || {
            r.recv().unwrap()?;
            let mut buffer = buffer.writer();
            {
                let range = source.slice(..).get_mapped_range();
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
