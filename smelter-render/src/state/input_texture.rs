use interleaved_uyvy422::InterleavedUyvy422Input;
use nv12_texture::NV12Input;
use planar_yuv::PlanarYuvInput;
use rgba_texture::RgbaTextureInput;

use crate::{
    BufferFormat, Frame, FrameData, Resolution, YuvPlanes,
    state::input_texture::{
        argb::ArgbInput, bgra::BgraInput, interleaved_yuyv422::InterleavedYuyv422Input,
    },
    wgpu::{WgpuCtx, texture::PlanarYuvVariant},
};

use super::node_texture::NodeTexture;

// GPU - connect as linear view
// CPU - connect as linear(default) view
// WebGl - create temporary rgb texture, write to it convert from srg to rgb

mod argb;
mod bgra;
mod interleaved_uyvy422;
mod interleaved_yuyv422;
mod nv12_texture;
mod planar_yuv;
mod rgba_texture;

mod convert_linear_to_srgb;

enum InputTextureState {
    PlanarYuv(Box<PlanarYuvInput>),
    InterleavedUyvy422(InterleavedUyvy422Input),
    InterleavedYuyv422(InterleavedYuyv422Input),
    Nv12(NV12Input),
    Bgra(BgraInput),
    Argb(ArgbInput),
    /// Depending on rendering mode
    /// - GPU - Rgba8UnormSrgb
    /// - CPU optimized - Rgba8Unorm (but data is in sRGB color space)
    /// - WebGl - Rgba8UnormSrgb
    Rgba8Unorm(RgbaTextureInput),
}

impl InputTextureState {
    fn resolution(&self) -> Resolution {
        match &self {
            InputTextureState::PlanarYuv(input) => input.resolution(),
            InputTextureState::InterleavedUyvy422(input) => input.resolution(),
            InputTextureState::InterleavedYuyv422(input) => input.resolution(),
            InputTextureState::Rgba8Unorm(input) => input.resolution(),
            InputTextureState::Nv12(input) => input.resolution(),
            InputTextureState::Bgra(input) => input.resolution(),
            InputTextureState::Argb(input) => input.resolution(),
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
        let resolution = frame.resolution;
        match frame.data {
            FrameData::PlanarYuv420(planes) => {
                self.planar_yuv(ctx, PlanarYuvVariant::YUV420, planes, resolution)
            }
            FrameData::PlanarYuv422(planes) => {
                self.planar_yuv(ctx, PlanarYuvVariant::YUV422, planes, resolution)
            }
            FrameData::PlanarYuv444(planes) => {
                self.planar_yuv(ctx, PlanarYuvVariant::YUV444, planes, resolution)
            }
            FrameData::PlanarYuvJ420(planes) => {
                self.planar_yuv(ctx, PlanarYuvVariant::YUVJ420, planes, resolution)
            }
            FrameData::Nv12(planes) => self.nv12(ctx, resolution).upload(ctx, planes, resolution),
            FrameData::InterleavedUyvy422(data) => self.uyvy(ctx).upload(ctx, &data, resolution),
            FrameData::InterleavedYuyv422(data) => self.yuyv(ctx).upload(ctx, &data, resolution),
            FrameData::Bgra(data) => self.bgra(ctx).upload(ctx, &data, resolution),
            FrameData::Argb(data) => self.argb(ctx).upload(ctx, &data, resolution),
            FrameData::Rgba8UnormWgpuTexture(texture) => match &mut self.0 {
                Some(InputTextureState::Rgba8Unorm(input)) => input.update(texture),
                state => {
                    *state = Some(InputTextureState::Rgba8Unorm(RgbaTextureInput::new(
                        texture,
                    )))
                }
            },
            FrameData::Nv12WgpuTexture(texture) => match &mut self.0 {
                Some(InputTextureState::Nv12(input)) => input.update(ctx, texture).unwrap(),
                state => {
                    *state = Some(InputTextureState::Nv12(
                        NV12Input::new_from_texture(ctx, texture).unwrap(),
                    ))
                }
            },
        }
    }

    /// The textures `format` frames of `resolution` upload into, one per plane.
    pub fn upload_textures(
        &mut self,
        ctx: &WgpuCtx,
        format: BufferFormat,
        resolution: Resolution,
    ) -> Vec<wgpu::Texture> {
        match format {
            BufferFormat::InterleavedUyvy422 => self.uyvy(ctx).upload_textures(ctx, resolution),
            BufferFormat::InterleavedYuyv422 => self.yuyv(ctx).upload_textures(ctx, resolution),
            BufferFormat::Nv12 => self.nv12(ctx, resolution).upload_textures(ctx, resolution),
            BufferFormat::Bgra => self.bgra(ctx).upload_textures(ctx, resolution),
        }
    }

    /// The current state, replaced by `create` unless it `is` the wanted one.
    fn ensure(
        &mut self,
        is: impl Fn(&InputTextureState) -> bool,
        create: impl FnOnce() -> InputTextureState,
    ) -> &mut InputTextureState {
        if !self.0.as_ref().is_some_and(is) {
            self.0 = Some(create());
        }
        self.0.as_mut().unwrap()
    }

    fn planar_yuv(
        &mut self,
        ctx: &WgpuCtx,
        variant: PlanarYuvVariant,
        planes: YuvPlanes,
        resolution: Resolution,
    ) {
        match self.ensure(
            |state| matches!(state, InputTextureState::PlanarYuv(_)),
            || InputTextureState::PlanarYuv(Box::new(PlanarYuvInput::new(ctx, variant))),
        ) {
            InputTextureState::PlanarYuv(input) => input.upload(ctx, planes, variant, resolution),
            _ => unreachable!(),
        }
    }

    fn nv12(&mut self, ctx: &WgpuCtx, resolution: Resolution) -> &mut NV12Input {
        match self.ensure(
            |state| matches!(state, InputTextureState::Nv12(_)),
            || InputTextureState::Nv12(NV12Input::new_uploadable(ctx, resolution)),
        ) {
            InputTextureState::Nv12(input) => input,
            _ => unreachable!(),
        }
    }

    fn uyvy(&mut self, ctx: &WgpuCtx) -> &mut InterleavedUyvy422Input {
        match self.ensure(
            |state| matches!(state, InputTextureState::InterleavedUyvy422(_)),
            || InputTextureState::InterleavedUyvy422(InterleavedUyvy422Input::new(ctx)),
        ) {
            InputTextureState::InterleavedUyvy422(input) => input,
            _ => unreachable!(),
        }
    }

    fn yuyv(&mut self, ctx: &WgpuCtx) -> &mut InterleavedYuyv422Input {
        match self.ensure(
            |state| matches!(state, InputTextureState::InterleavedYuyv422(_)),
            || InputTextureState::InterleavedYuyv422(InterleavedYuyv422Input::new(ctx)),
        ) {
            InputTextureState::InterleavedYuyv422(input) => input,
            _ => unreachable!(),
        }
    }

    fn bgra(&mut self, ctx: &WgpuCtx) -> &mut BgraInput {
        match self.ensure(
            |state| matches!(state, InputTextureState::Bgra(_)),
            || InputTextureState::Bgra(BgraInput::new(ctx)),
        ) {
            InputTextureState::Bgra(input) => input,
            _ => unreachable!(),
        }
    }

    fn argb(&mut self, ctx: &WgpuCtx) -> &mut ArgbInput {
        match self.ensure(
            |state| matches!(state, InputTextureState::Argb(_)),
            || InputTextureState::Argb(ArgbInput::new(ctx)),
        ) {
            InputTextureState::Argb(input) => input,
            _ => unreachable!(),
        }
    }

    pub fn convert_to_node_texture(&mut self, ctx: &WgpuCtx, dest: &mut NodeTexture) {
        match &mut self.0 {
            Some(input_texture) => {
                let dst_state = dest.ensure_size(ctx, input_texture.resolution());
                match input_texture {
                    InputTextureState::PlanarYuv(state) => state.convert(ctx, dst_state),
                    InputTextureState::InterleavedUyvy422(state) => state.convert(ctx, dst_state),
                    InputTextureState::InterleavedYuyv422(state) => state.convert(ctx, dst_state),
                    InputTextureState::Rgba8Unorm(state) => state.convert(ctx, dst_state),
                    InputTextureState::Nv12(state) => state.convert(ctx, dst_state),
                    InputTextureState::Bgra(state) => state.convert(ctx, dst_state),
                    InputTextureState::Argb(state) => state.convert(ctx, dst_state),
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
