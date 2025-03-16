use std::{io::Write, mem, sync::Arc};

use bytes::{BufMut, Bytes, BytesMut};
use crossbeam_channel::bounded;
use log::error;
use wgpu::{Buffer, BufferAsyncError, MapMode};

use crate::{Frame, FrameData, Resolution, YuvPlanes};

use self::utils::{pad_to_256, texture_size_to_resolution};

use super::WgpuCtx;

mod base;
mod bgra;
mod interleaved_yuv422;
mod nv12;
mod planar_yuv;
mod rgba;
pub mod utils;

pub type BGRATexture = bgra::BGRATexture;
pub type RGBATexture = rgba::RGBATexture;
pub type PlanarYuvTextures = planar_yuv::PlanarYuvTextures;
pub type PlanarYuvVariant = planar_yuv::YuvVariant;
pub type InterleavedYuv422Texture = interleaved_yuv422::InterleavedYuv422Texture;
pub type NV12TextureView<'a> = nv12::NV12TextureView<'a>;

pub use base::TextureExt;
pub use planar_yuv::YuvPendingDownload as PlanarYuvPendingDownload;

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
            | InputTextureState::Nv12WgpuTexture(texture) => {
                let size = texture.size();
                Resolution {
                    width: size.width as usize,
                    height: size.height as usize,
                }
            }
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
            FrameData::PlanarYuv420(planes) => self.upload_planar_yuv(
                ctx,
                planes,
                frame.resolution,
                planar_yuv::YuvVariant::YUV420,
            ),
            FrameData::PlanarYuvJ420(planes) => self.upload_planar_yuv(
                ctx,
                planes,
                frame.resolution,
                planar_yuv::YuvVariant::YUVJ420,
            ),
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
        variant: planar_yuv::YuvVariant,
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
            let bind_group = textures.new_bind_group(ctx, ctx.format.planar_yuv_layout());
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
            let bind_group = texture.new_bind_group(ctx, ctx.format.interleaved_yuv_layout());

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
                    } => ctx.format.convert_planar_yuv_to_rgba(
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
                        let bind_group = texture.new_bind_group(ctx, ctx.format.nv12_layout());
                        ctx.format.convert_nv12_to_rgba(
                            ctx,
                            (&texture, &bind_group),
                            dest_state.rgba_texture(),
                        )
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

pub struct NodeTextureState {
    texture: RGBATexture,
    raw_bind_group: wgpu::BindGroup,
}

impl NodeTextureState {
    fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let texture = RGBATexture::new(ctx, resolution);
        let raw_bind_group = texture.new_raw_bind_group(ctx, ctx.format.rgba_layout());

        Self {
            texture,
            raw_bind_group,
        }
    }

    pub fn rgba_texture(&self) -> &RGBATexture {
        &self.texture
    }

    pub fn raw_bind_group(&self) -> &wgpu::BindGroup {
        &self.raw_bind_group
    }

    pub fn resolution(&self) -> Resolution {
        texture_size_to_resolution(&self.texture.size())
    }
}

pub struct NodeTexture(OptionalState<NodeTextureState>);

impl NodeTexture {
    pub fn new() -> Self {
        Self(OptionalState::new())
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn ensure_size<'a>(
        &'a mut self,
        ctx: &WgpuCtx,
        new_resolution: Resolution,
    ) -> &'a NodeTextureState {
        self.0 = match self.0.replace(OptionalState::None) {
            OptionalState::NoneWithOldState(state) | OptionalState::Some(state) => {
                if texture_size_to_resolution(&state.texture.size()) == new_resolution {
                    OptionalState::Some(state)
                } else {
                    let new_inner = NodeTextureState::new(ctx, new_resolution);
                    OptionalState::Some(new_inner)
                }
            }
            OptionalState::None => {
                let new_inner = NodeTextureState::new(ctx, new_resolution);
                OptionalState::Some(new_inner)
            }
        };
        self.0.state().unwrap()
    }

    pub fn state(&self) -> Option<&NodeTextureState> {
        self.0.state()
    }

    pub fn resolution(&self) -> Option<Resolution> {
        self.0.state().map(NodeTextureState::resolution)
    }

    pub fn texture(&self) -> Option<&RGBATexture> {
        self.state().map(|state| state.rgba_texture())
    }
}

impl Default for NodeTexture {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Type that behaves like Option, but when is set to None
/// it keeps ownership of the value it had before.
enum OptionalState<State> {
    None,
    /// It should be treated as None, but hold on the old state, so
    /// it can be reused in the future.
    NoneWithOldState(State),
    Some(State),
}

impl<State> OptionalState<State> {
    fn new() -> Self {
        Self::None
    }

    fn clear(&mut self) {
        *self = match self.replace(Self::None) {
            Self::None => Self::None,
            Self::NoneWithOldState(state) => Self::NoneWithOldState(state),
            Self::Some(state) => Self::NoneWithOldState(state),
        }
    }

    fn state(&self) -> Option<&State> {
        match self {
            OptionalState::None => None,
            OptionalState::NoneWithOldState(_) => None,
            OptionalState::Some(ref state) => Some(state),
        }
    }

    fn replace(&mut self, replacement: Self) -> Self {
        mem::replace(self, replacement)
    }
}

impl<State> Default for OptionalState<State> {
    fn default() -> Self {
        Self::None
    }
}
