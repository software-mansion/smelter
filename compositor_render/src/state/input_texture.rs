use interleaved_yuv422::InterleavedYuv422Input;
use nv12_texture::NV12TextureInput;
use planar_yuv::PlanarYuvInput;
use rgba_texture::RgbaTextureInput;

use crate::{
    wgpu::{texture::PlanarYuvVariant, WgpuCtx},
    Frame, FrameData, Resolution,
};

use super::node_texture::NodeTexture;

// GPU - connect as linear view
// CPU - connect as linear(default) view
// WebGl - create temporary rgb texture, write to it convert from srg to rgb

mod interleaved_yuv422;
mod nv12_texture;
mod planar_yuv;
mod rgba_texture;

mod convert_linear_to_srgb;

enum InputTextureState {
    PlanarYuvTextures(PlanarYuvInput),
    InterleavedYuv422Texture(InterleavedYuv422Input),
    Nv12WgpuTexture(NV12TextureInput),
    /// Depending on rendering mode
    /// - GPU - Rgba8UnormSrgb
    /// - CPU optimized - Rgba8Unorm (but data is in sRGB color space)
    /// - WebGl - Rgba8UnormSrgb
    Rgba8UnormWgpuTexture(RgbaTextureInput),
}

impl InputTextureState {
    fn resolution(&self) -> Resolution {
        match &self {
            InputTextureState::PlanarYuvTextures(input) => input.resolution(),
            InputTextureState::InterleavedYuv422Texture(input) => input.resolution(),
            InputTextureState::Rgba8UnormWgpuTexture(input) => input.resolution(),
            InputTextureState::Nv12WgpuTexture(input) => input.resolution(),
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
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuvTextures(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV420, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV420);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV420, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuvTextures(input));
                    }
                };
            }
            FrameData::PlanarYuv422(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuvTextures(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV422, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV422);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV422, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuvTextures(input));
                    }
                };
            }
            FrameData::PlanarYuv444(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuvTextures(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV444, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV444);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV444, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuvTextures(input));
                    }
                };
            }
            FrameData::PlanarYuvJ420(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuvTextures(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUVJ420, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUVJ420);
                        input.upload(ctx, planes, PlanarYuvVariant::YUVJ420, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuvTextures(input));
                    }
                };
            }
            FrameData::InterleavedYuv422(data) => {
                match &mut self.0 {
                    Some(InputTextureState::InterleavedYuv422Texture(input)) => {
                        input.upload(ctx, &data, frame.resolution);
                    }
                    state => {
                        let mut input = InterleavedYuv422Input::new(ctx);
                        input.upload(ctx, &data, frame.resolution);
                        *state = Some(InputTextureState::InterleavedYuv422Texture(input));
                    }
                };
            }
            FrameData::Rgba8UnormWgpuTexture(texture) => {
                match &mut self.0 {
                    Some(InputTextureState::Rgba8UnormWgpuTexture(input)) => {
                        input.update(texture);
                    }
                    state => {
                        *state = Some(InputTextureState::Rgba8UnormWgpuTexture(
                            RgbaTextureInput::new(texture),
                        ));
                    }
                };
            }
            FrameData::Nv12WgpuTexture(texture) => {
                match &mut self.0 {
                    Some(InputTextureState::Nv12WgpuTexture(input)) => {
                        input.update(ctx, texture).unwrap();
                    }
                    state => {
                        *state = Some(InputTextureState::Nv12WgpuTexture(
                            NV12TextureInput::new(ctx, texture).unwrap(),
                        ));
                    }
                };
            }
        }
    }

    pub fn convert_to_node_texture(&mut self, ctx: &WgpuCtx, dest: &mut NodeTexture) {
        match &mut self.0 {
            Some(input_texture) => {
                let dst_state = dest.ensure_size(ctx, input_texture.resolution());
                match input_texture {
                    InputTextureState::PlanarYuvTextures(state) => state.convert(ctx, dst_state),
                    InputTextureState::InterleavedYuv422Texture(state) => {
                        state.convert(ctx, dst_state)
                    }
                    InputTextureState::Rgba8UnormWgpuTexture(state) => {
                        state.convert(ctx, dst_state)
                    }
                    InputTextureState::Nv12WgpuTexture(state) => state.convert(ctx, dst_state),
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
