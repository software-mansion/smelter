use std::sync::Arc;

use crate::{
    wgpu::{
        texture::{InterleavedYuv422Texture, PlanarYuvTextures, PlanarYuvVariant},
        WgpuCtx,
    },
    Frame, FrameData, Resolution, YuvPlanes,
};

// GPU - connect as linear view
// CPU - connect as linear(default) view
// WebGl - create temporary rgb texture, write to it convert from srg to rgb

mod planar_yuv420;
mod rgb_to_srgb;

enum InputTextureState {
    PlanarYuvTextures {
        textures: PlanarYuvTextures,
        bind_group: wgpu::BindGroup,
    },
    InterleavedYuv422Texture {
        texture: InterleavedYuv422Texture,
        bind_group: wgpu::BindGroup,
    },
    Rgba8UnormWgpuTexture(Arc<wgpu::Texture>),
    Nv12WgpuTexture(Arc<wgpu::Texture>),
}

impl InputTextureState {
    fn resolution(&self) -> Resolution {
        match &self {
            InputTextureState::PlanarYuvTextures { textures, .. } => textures.resolution,
            InputTextureState::InterleavedYuv422Texture { texture, .. } => texture.resolution,
            InputTextureState::Rgba8UnormWgpuTexture(texture)
            | InputTextureState::Nv12WgpuTexture(texture) => texture.size().into(),
        }
    }
}

pub struct InputTexture(Option<InputTextureState>);

impl InputTexture {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn upload(&mut self, ctx: &WgpuCtx, frame: Frame) {
        match frame.data {
            FrameData::PlanarYuv420(planes) => {
                self.upload_planar_yuv(ctx, planes, frame.resolution, PlanarYuvVariant::YUV420)
            }
            FrameData::PlanarYuvJ420(planes) => {
                self.upload_planar_yuv(ctx, planes, frame.resolution, PlanarYuvVariant::YUVJ420)
            }
            FrameData::InterleavedYuv422(data) => {
                self.upload_interleaved_yuv(ctx, data, frame.resolution)
            }
            FrameData::Rgba8UnormWgpuTexture(texture) => {
                self.0 = Some(InputTextureState::Rgba8UnormWgpuTexture(texture))
            }
            FrameData::Nv12WgpuTexture(texture) => {
                self.0 = Some(InputTextureState::Nv12WgpuTexture(texture))
            }
        }
    }

    fn upload_planar_yuv(
        &mut self,
        ctx: &WgpuCtx,
        planes: YuvPlanes,
        resolution: Resolution,
        variant: PlanarYuvVariant,
    ) {
        let should_recreate = match &self.0 {
            Some(state) => {
                !matches!(state, InputTextureState::PlanarYuvTextures { .. })
                    || resolution != state.resolution()
            }
            None => true,
        };

        if should_recreate {
            let textures = PlanarYuvTextures::new(ctx, resolution);
            let bind_group = textures.new_bind_group(ctx, &ctx.format.planar_yuv_layout);
            self.0 = Some(InputTextureState::PlanarYuvTextures {
                textures,
                bind_group,
            })
        }
        let Some(InputTextureState::PlanarYuvTextures { textures, .. }) = self.0.as_mut() else {
            error!("Invalid texture format.");
            return;
        };
        textures.upload(ctx, &planes, variant)
    }

    fn upload_interleaved_yuv(
        &mut self,
        ctx: &WgpuCtx,
        data: bytes::Bytes,
        resolution: Resolution,
    ) {
        let should_recreate = match &self.0 {
            Some(state) => {
                !matches!(state, InputTextureState::InterleavedYuv422Texture { .. })
                    || resolution != state.resolution()
            }
            None => true,
        };

        if should_recreate {
            let texture = InterleavedYuv422Texture::new(ctx, resolution);
            let bind_group = texture.new_bind_group(ctx, &ctx.format.single_texture_layout);

            self.0 = Some(InputTextureState::InterleavedYuv422Texture {
                texture,
                bind_group,
            });
        }

        let Some(InputTextureState::InterleavedYuv422Texture { texture, .. }) = self.0.as_mut()
        else {
            error!("Invalid texture format.");
            return;
        };
        texture.upload(ctx, &data)
    }

    pub fn convert_to_node_texture(&self, ctx: &WgpuCtx, dest: &mut NodeTexture) {
        match &self.0 {
            Some(input_texture) => {
                let dest_state = dest.ensure_size(ctx, input_texture.resolution());
                match &input_texture {
                    InputTextureState::PlanarYuvTextures {
                        textures,
                        bind_group,
                    } => ctx.format.planar_yuv_to_rgba_srgb.convert(
                        ctx,
                        (textures, bind_group),
                        dest_state.rgba_texture(),
                    ),
                    InputTextureState::InterleavedYuv422Texture {
                        texture,
                        bind_group,
                    } => ctx.format.convert_interleaved_yuv_to_rgba(
                        ctx,
                        (texture, bind_group),
                        dest_state.rgba_texture(),
                    ),
                    InputTextureState::Rgba8UnormWgpuTexture(texture) => {
                        if let Err(err) = dest_state
                            .rgba_texture()
                            .texture()
                            .fill_from_wgpu_texture(ctx, texture)
                        {
                            error!("Invalid texture passed as an input: {err}")
                        }
                    }
                    InputTextureState::Nv12WgpuTexture(texture) => {
                        let texture = match NV12TextureView::from_wgpu_texture(texture.as_ref()) {
                            Ok(texture) => texture,
                            Err(err) => {
                                error!("Invalid texture passed as input: {err}");
                                return;
                            }
                        };
                        let bind_group = texture.new_bind_group(ctx, &ctx.format.nv12_layout);
                        ctx.format
                            .nv12_to_rgba
                            .convert(ctx, &bind_group, dest_state.rgba_texture())
                    }
                }
            }
            None => dest.clear(),
        }
    }
}

impl Default for InputTexture {
    fn default() -> Self {
        Self::new()
    }
}
