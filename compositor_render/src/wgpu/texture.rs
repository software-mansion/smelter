use std::io::Write;

use bytes::{BufMut, Bytes, BytesMut};
use crossbeam_channel::bounded;
use log::error;
use wgpu::{Buffer, BufferAsyncError, MapMode};

use crate::Resolution;

use self::utils::pad_to_256;

use super::WgpuCtx;

mod base;
mod bgra;
mod interleaved_yuv422;
mod nv12;
mod planar_yuv;
mod rgba_linear;
mod rgba_multiview;
mod rgba_srgb;
pub mod utils;

pub type BGRATexture = bgra::BGRATexture;
pub type RgbaMultiViewTexture = rgba_multiview::RgbaMultiViewTexture;
pub type RgbaLinearTexture = rgba_linear::RgbaLinearTexture;
pub type RgbaSrgbTexture = rgba_srgb::RgbaSrgbTexture;
pub type PlanarYuvTextures = planar_yuv::PlanarYuvTextures;
pub type PlanarYuvVariant = planar_yuv::YuvVariant;
pub type InterleavedYuv422Texture = interleaved_yuv422::InterleavedYuv422Texture;
pub type NV12Texture = nv12::NV12Texture;

pub use base::TextureExt;
pub use nv12::NV12TextureViewCreateError;
pub use planar_yuv::YuvPendingDownload as PlanarYuvPendingDownload;

pub struct OutputTexture {
    textures: PlanarYuvTextures,
    buffers: [wgpu::Buffer; 3],
    resolution: Resolution,
}

impl OutputTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let textures = PlanarYuvTextures::new(ctx, resolution);
        let buffers = textures.new_download_buffers(ctx);

        Self {
            textures,
            buffers,
            resolution: resolution.to_owned(),
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
        impl FnOnce() -> Result<Bytes, BufferAsyncError> + 'a,
        BufferAsyncError,
    > {
        self.textures.copy_to_buffers(ctx, &self.buffers);

        PlanarYuvPendingDownload::new(
            self.download_buffer(self.textures.planes_textures[0].size(), &self.buffers[0]),
            self.download_buffer(self.textures.planes_textures[1].size(), &self.buffers[1]),
            self.download_buffer(self.textures.planes_textures[2].size(), &self.buffers[2]),
        )
    }

    fn download_buffer<'a>(
        &'a self,
        size: wgpu::Extent3d,
        source: &'a Buffer,
    ) -> impl FnOnce() -> Result<Bytes, BufferAsyncError> + 'a {
        let buffer = BytesMut::with_capacity((size.width * size.height) as usize);
        let (s, r) = bounded(1);
        source.slice(..).map_async(MapMode::Read, move |result| {
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
