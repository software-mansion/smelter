use std::io::Write;
use std::sync::Arc;

use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;

use crate::{
    Frame, RenderingMode, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{RgbaLinearTexture, RgbaSrgbTexture, TextureExt, utils::pad_to_256},
    },
};

use super::{input_texture::InputTexture, node_texture::NodeTexture};

/// Stand-alone single-frame processor. Takes one `Frame` and runs it through
/// a fixed pipeline:
///
///   1. upload: `Frame` (source format) -> `input_texture`
///   2. convert: `input_texture` -> `node_texture` (RGBA at source resolution)
///   3. rescale (optional, only when a target resolution is requested):
///      `node_texture` -> `rescale_texture` (RGBA at target resolution)
///   4. output - one of:
///        - `process_to_texture`: clone the final RGBA texture and return it
///        - `process_to_bytes`: copy the final RGBA texture to
///          `download_buffer`, map it, and return tightly-packed bytes
///
/// All intermediate textures and the readback buffer are owned by the
/// instance and reused across calls; they are recreated only when the
/// requested resolution changes.
pub struct FramePreProcessor {
    wgpu_ctx: Arc<WgpuCtx>,
    /// Raw uploaded frame in its source format (YUV / NV12 / RGBA / …).
    input_texture: InputTexture,
    /// RGBA texture at the source frame's resolution.
    node_texture: NodeTexture,
    /// RGBA texture at the caller-requested resolution. Only populated when
    /// rescaling is needed. Storage format matches `node_texture` (linear for
    /// `CpuOptimized`, sRGB for `GpuOptimized`/`WebGl`) so the two branches
    /// of `process_to_*` return bytes with the same encoding.
    rescale_texture: Option<RescaleTexture>,
    /// Staging buffer used by `process_to_bytes` to read the final texture
    /// back to CPU memory.
    download_buffer: Option<(wgpu::Buffer, Resolution)>,
}

impl FramePreProcessor {
    pub fn new(wgpu_ctx: Arc<WgpuCtx>) -> Self {
        Self {
            wgpu_ctx,
            input_texture: InputTexture::new(),
            node_texture: NodeTexture::new(),
            rescale_texture: None,
            download_buffer: None,
        }
    }

    pub fn process_to_texture(
        &mut self,
        frame: Frame,
        resolution: Option<Resolution>,
    ) -> Arc<wgpu::Texture> {
        self.upload_and_convert_to_node_texture(frame);

        let output_texture_views: &'static [wgpu::TextureFormat] = match self.wgpu_ctx.mode {
            RenderingMode::GpuOptimized => &[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ],
            RenderingMode::CpuOptimized => &[wgpu::TextureFormat::Rgba8Unorm],
            RenderingMode::WebGl => &[wgpu::TextureFormat::Rgba8UnormSrgb],
        };

        let texture = match resolution {
            Some(resolution) => {
                self.rescale_node_texture(resolution);
                self.rescale_texture.as_ref().unwrap().texture()
            }
            None => self.node_texture.state().unwrap().texture(),
        };

        Arc::new(texture.clone_texture(&self.wgpu_ctx, output_texture_views))
    }

    pub fn process_to_bytes(
        &mut self,
        frame: Frame,
        resolution: Option<Resolution>,
    ) -> bytes::Bytes {
        self.upload_and_convert_to_node_texture(frame);

        if let Some(resolution) = resolution {
            self.rescale_node_texture(resolution);
            self.ensure_download_buffer(resolution);

            let rescale_texture = self.rescale_texture.as_ref().unwrap().texture();
            self.download(rescale_texture, resolution)
        } else {
            let resolution = self.node_texture.resolution().unwrap();
            self.ensure_download_buffer(resolution);

            let node_texture = self.node_texture.state().unwrap().texture();
            self.download(node_texture, resolution)
        }
    }

    fn upload_and_convert_to_node_texture(&mut self, frame: Frame) {
        self.input_texture.upload(&self.wgpu_ctx, frame);
        // Flush uploads before the conversion pass reads them.
        self.wgpu_ctx.queue.submit([]);
        self.input_texture
            .convert_to_node_texture(&self.wgpu_ctx, &mut self.node_texture);
    }

    fn rescale_node_texture(&mut self, resolution: Resolution) {
        let node_state = self.node_texture.state().unwrap();
        // Use the bind group that decodes sRGB on sample so bilinear filtering
        // runs in linear space.
        let src_bg = node_state.sampling_bind_group();

        let needs_recreate = match &self.rescale_texture {
            Some(tex) => Resolution::from(tex.size()) != resolution,
            None => true,
        };
        if needs_recreate {
            self.rescale_texture = Some(RescaleTexture::new(&self.wgpu_ctx, resolution));
        }
        let rescale_texture = self.rescale_texture.as_ref().unwrap();
        rescale_texture.rescale(&self.wgpu_ctx, src_bg);
    }

    fn ensure_download_buffer(&mut self, resolution: Resolution) {
        let needs_recreate = match &self.download_buffer {
            Some((_, buf_res)) => *buf_res != resolution,
            None => true,
        };
        if needs_recreate {
            let size = wgpu::Extent3d {
                width: resolution.width as u32,
                height: resolution.height as u32,
                depth_or_array_layers: 1,
            };
            // Row stride must be a multiple of 256 for texture-to-buffer copy.
            let bytes_per_row = pad_to_256(size.width * 4);
            let buffer = self.wgpu_ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame pre-processor download buffer"),
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                size: (bytes_per_row * size.height) as u64,
            });
            self.download_buffer = Some((buffer, resolution));
        }
    }

    fn download(&self, texture: &wgpu::Texture, resolution: Resolution) -> bytes::Bytes {
        let (buffer, _) = self.download_buffer.as_ref().unwrap();

        let mut encoder =
            self.wgpu_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("RGBA download encoder"),
                });
        texture.copy_to_buffer(&mut encoder, buffer);
        self.wgpu_ctx.queue.submit(Some(encoder.finish()));

        let (s, r) = bounded(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if let Err(err) = s.send(result) {
                    error!("channel send error: {err}");
                }
            });

        while let Err(wgpu::PollError::Timeout) = self
            .wgpu_ctx
            .device
            .poll(wgpu::PollType::wait_indefinitely())
        {}

        r.recv().unwrap().unwrap();

        // Strip the 256-byte row padding so the output is tightly packed.
        let width = resolution.width as u32;
        let row_bytes = (width * 4) as usize;
        let padded_row_bytes = pad_to_256(width * 4) as usize;

        let output = bytes::BytesMut::with_capacity(row_bytes * resolution.height);
        let mut writer = output.writer();

        {
            let range = buffer.slice(..).get_mapped_range();
            for chunk in range.chunks(padded_row_bytes) {
                writer.write_all(&chunk[..row_bytes]).unwrap();
            }
        }
        buffer.unmap();

        writer.into_inner().into()
    }
}

enum RescaleTexture {
    Linear(RgbaLinearTexture),
    Srgb(RgbaSrgbTexture),
}

impl RescaleTexture {
    fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        match ctx.mode {
            RenderingMode::CpuOptimized => Self::Linear(RgbaLinearTexture::new(ctx, resolution)),
            RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                Self::Srgb(RgbaSrgbTexture::new(ctx, resolution))
            }
        }
    }

    fn texture(&self) -> &wgpu::Texture {
        match self {
            Self::Linear(t) => t.texture(),
            Self::Srgb(t) => t.texture(),
        }
    }

    fn view(&self) -> &wgpu::TextureView {
        match self {
            Self::Linear(t) => t.view(),
            Self::Srgb(t) => t.view(),
        }
    }

    fn size(&self) -> wgpu::Extent3d {
        self.texture().size()
    }

    /// Rescale `src_bg` into `self` using the rescaler that matches this
    /// texture's format. `src_bg` must sample in linear space so filtering
    /// is correct; the sRGB variant re-encodes on write.
    fn rescale(&self, ctx: &WgpuCtx, src_bg: &wgpu::BindGroup) {
        let rescaler = match self {
            Self::Linear(_) => &ctx.format.rgba_rescale_linear,
            Self::Srgb(_) => &ctx.format.rgba_rescale_srgb,
        };
        rescaler.convert(ctx, src_bg, self.view());
    }
}
