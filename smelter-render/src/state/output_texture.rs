use std::{io::Write, time::Duration};

use bytes::{BufMut, Bytes};
use crossbeam_channel::bounded;
use tracing::error;
use wgpu::{Buffer, BufferAsyncError};

use crate::{
    Frame, FrameData, OutputFrameFormat, RenderingMode, Resolution,
    scene::RGBColor,
    wgpu::{
        WgpuCtx,
        resampler::{Crop, Lanczos3TextureCache},
        texture::{
            NV12Texture, PlanarYuvPendingDownload, PlanarYuvTextures, PlanarYuvVariant,
            RgbaLinearTexture, RgbaMultiViewTexture, RgbaSrgbTexture, TextureExt,
            utils::pad_to_256,
        },
    },
};

use super::{
    RenderCtx,
    node_texture::{NodeTexture, NodeTextureState},
};

pub struct OutputTexture {
    resolution: Resolution,
    kind: OutputTextureKind,
    resampler: OutputResampler,
}

enum OutputTextureKind {
    PlanarYuvTextures(Box<PlanarYuvOutput>),
    Rgba8UnormWgpuTexture,
    Nv12WgpuTexture,
}

impl OutputTexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution, format: OutputFrameFormat) -> Self {
        let kind = match format {
            OutputFrameFormat::PlanarYuv420Bytes => OutputTextureKind::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV420),
            )),
            OutputFrameFormat::PlanarYuv422Bytes => OutputTextureKind::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV422),
            )),
            OutputFrameFormat::PlanarYuv444Bytes => OutputTextureKind::PlanarYuvTextures(Box::new(
                PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV444),
            )),
            OutputFrameFormat::RgbaWgpuTexture => OutputTextureKind::Rgba8UnormWgpuTexture,
            OutputFrameFormat::Nv12WgpuTexture => OutputTextureKind::Nv12WgpuTexture,
        };
        Self {
            resolution,
            kind,
            resampler: OutputResampler::default(),
        }
    }

    /// Converts `source` to the output format, or produces a black frame if there is no source.
    /// Source with a different resolution is stretched to the output resolution.
    pub fn read<'a>(
        &'a mut self,
        ctx: &RenderCtx,
        source: Option<&NodeTextureState>,
        pts: Duration,
    ) -> PartialOutputFrame<'a, impl FnOnce() -> Result<Bytes, BufferAsyncError> + 'a> {
        let resolution = self.resolution;
        let source = source.map(|source| self.resampler.resample(ctx, source, resolution));
        let wgpu_ctx = ctx.wgpu_ctx;
        match &self.kind {
            OutputTextureKind::PlanarYuvTextures(yuv_output) => {
                let yuv_textures = yuv_output.yuv_textures();
                match source {
                    Some(node) => wgpu_ctx.format.rgba_to_yuv.convert(
                        wgpu_ctx,
                        node.output_texture_bind_group(),
                        yuv_textures,
                    ),
                    None => yuv_textures.fill_with_color(wgpu_ctx, RGBColor::BLACK),
                }
                PartialOutputFrame::PendingYuvDownload {
                    pending_download: yuv_output.start_download(wgpu_ctx),
                    variant: yuv_textures.variant(),
                    resolution: yuv_output.resolution(),
                    pts,
                }
            }
            OutputTextureKind::Rgba8UnormWgpuTexture => {
                let texture = match (source, wgpu_ctx.mode) {
                    (Some(node), RenderingMode::GpuOptimized) => node.texture().clone_texture(
                        wgpu_ctx,
                        &[
                            wgpu::TextureFormat::Rgba8Unorm,
                            wgpu::TextureFormat::Rgba8UnormSrgb,
                        ],
                    ),
                    (Some(node), RenderingMode::CpuOptimized) => node
                        .texture()
                        .clone_texture(wgpu_ctx, &[wgpu::TextureFormat::Rgba8Unorm]),
                    (Some(node), RenderingMode::WebGl) => node
                        .texture()
                        .clone_texture(wgpu_ctx, &[wgpu::TextureFormat::Rgba8UnormSrgb]),
                    (None, RenderingMode::GpuOptimized) => {
                        RgbaMultiViewTexture::new(wgpu_ctx, resolution).texture_owned()
                    }
                    (None, RenderingMode::CpuOptimized) => {
                        RgbaLinearTexture::new(wgpu_ctx, resolution).texture_owned()
                    }
                    (None, RenderingMode::WebGl) => {
                        RgbaSrgbTexture::new(wgpu_ctx, resolution).texture_owned()
                    }
                };
                PartialOutputFrame::CompleteFrame(Frame {
                    resolution: texture.size().into(),
                    data: FrameData::Rgba8UnormWgpuTexture(texture),
                    pts,
                })
            }
            OutputTextureKind::Nv12WgpuTexture => {
                let texture = NV12Texture::new(wgpu_ctx, resolution);
                match source {
                    Some(node) => wgpu_ctx.format.rgba_to_nv12.convert(
                        wgpu_ctx,
                        node.output_texture_bind_group(),
                        &texture,
                    ),
                    None => texture.fill_with_color(wgpu_ctx, RGBColor::BLACK),
                }
                PartialOutputFrame::CompleteFrame(Frame {
                    resolution,
                    data: FrameData::Nv12WgpuTexture(texture.texture().clone()),
                    pts,
                })
            }
        }
    }
}

/// Bilinear in CPU-optimized rendering, Lanczos3 otherwise.
#[derive(Default)]
struct OutputResampler {
    texture_cache: Lanczos3TextureCache,
    output: NodeTexture,
}

impl OutputResampler {
    fn resample<'a>(
        &'a mut self,
        ctx: &RenderCtx,
        source: &'a NodeTextureState,
        dst: Resolution,
    ) -> &'a NodeTextureState {
        let source_resolution = source.resolution();
        if source_resolution == dst {
            *self = Self::default();
            return source;
        }
        let resamplers = &ctx.wgpu_ctx.resamplers;
        let target = self.output.ensure_size(ctx.wgpu_ctx, dst);
        let mut encoder =
            ctx.wgpu_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Output resampler"),
                });
        match ctx.wgpu_ctx.mode {
            RenderingMode::CpuOptimized => {
                resamplers.bilinear_linear.resample(
                    ctx.wgpu_ctx,
                    source.view(),
                    target.view(),
                    &mut encoder,
                );
            }
            RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                let crop = Crop {
                    top: 0.0,
                    left: 0.0,
                    width: source_resolution.width as f32,
                    height: source_resolution.height as f32,
                };
                resamplers.lanczos3_srgb.resample(
                    ctx.wgpu_ctx,
                    source.view(),
                    crop,
                    target.view(),
                    &mut self.texture_cache,
                    &mut encoder,
                );
            }
        }
        ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
        target
    }
}

pub enum PartialOutputFrame<'a, F>
where
    F: FnOnce() -> Result<Bytes, BufferAsyncError> + 'a,
{
    PendingYuvDownload {
        pending_download: PlanarYuvPendingDownload<'a, F, BufferAsyncError>,
        variant: PlanarYuvVariant,
        resolution: Resolution,
        pts: Duration,
    },
    CompleteFrame(Frame),
}

impl<'a, F> PartialOutputFrame<'a, F>
where
    F: FnOnce() -> Result<Bytes, BufferAsyncError> + 'a,
{
    /// `device.poll` needs to be called after `OutputTexture::read`, but before this method.
    pub fn finish(self) -> Option<Frame> {
        match self {
            Self::PendingYuvDownload {
                pending_download,
                variant,
                resolution,
                pts,
            } => {
                let yuv_planes = match pending_download.wait() {
                    Ok(data) => data,
                    Err(err) => {
                        error!("Failed to download frame: {}", err);
                        return None;
                    }
                };
                let data = match variant {
                    PlanarYuvVariant::YUV420 => FrameData::PlanarYuv420(yuv_planes),
                    PlanarYuvVariant::YUV422 => FrameData::PlanarYuv422(yuv_planes),
                    PlanarYuvVariant::YUV444 => FrameData::PlanarYuv444(yuv_planes),
                    PlanarYuvVariant::YUVJ420 => FrameData::PlanarYuvJ420(yuv_planes),
                };
                Some(Frame {
                    data,
                    resolution,
                    pts,
                })
            }
            Self::CompleteFrame(frame) => Some(frame),
        }
    }
}

pub struct PlanarYuvOutput {
    textures: PlanarYuvTextures,
    buffers: [wgpu::Buffer; 3],
    resolution: Resolution,
}

impl PlanarYuvOutput {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution, pixel_format: PlanarYuvVariant) -> Self {
        let textures = PlanarYuvTextures::new(ctx, resolution, pixel_format);
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
