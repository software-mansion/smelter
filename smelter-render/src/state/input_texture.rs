use interleaved_uyvy422::InterleavedUyvy422Input;
use nv12_texture::NV12Input;
use planar_yuv::PlanarYuvInput;
use rgba_texture::RgbaTextureInput;

use crate::{
    Frame, FrameData, Resolution,
    state::input_texture::interleaved_yuyv422::InterleavedYuyv422Input,
    wgpu::{WgpuCtx, texture::PlanarYuvVariant},
};

use super::node_texture::NodeTexture;

// GPU - connect as linear view
// CPU - connect as linear(default) view
// WebGl - create temporary rgb texture, write to it convert from srg to rgb

mod interleaved_uyvy422;
mod interleaved_yuyv422;
mod nv12_texture;
mod planar_yuv;
mod rgba_texture;

mod convert_linear_to_srgb;

enum InputTextureState {
    PlanarYuv(PlanarYuvInput),
    InterleavedUyvy422(InterleavedUyvy422Input),
    InterleavedYuyv422(InterleavedYuyv422Input),
    Nv12(NV12Input),
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
                    Some(InputTextureState::PlanarYuv(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV420, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV420);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV420, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuv(input));
                    }
                };
            }
            FrameData::PlanarYuv422(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuv(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV422, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV422);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV422, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuv(input));
                    }
                };
            }
            FrameData::PlanarYuv444(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuv(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUV444, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUV444);
                        input.upload(ctx, planes, PlanarYuvVariant::YUV444, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuv(input));
                    }
                };
            }
            FrameData::PlanarYuvJ420(planes) => {
                match &mut self.0 {
                    Some(InputTextureState::PlanarYuv(input)) => {
                        input.upload(ctx, planes, PlanarYuvVariant::YUVJ420, frame.resolution);
                    }
                    state => {
                        let mut input = PlanarYuvInput::new(ctx, PlanarYuvVariant::YUVJ420);
                        input.upload(ctx, planes, PlanarYuvVariant::YUVJ420, frame.resolution);
                        *state = Some(InputTextureState::PlanarYuv(input));
                    }
                };
            }
            FrameData::Nv12(planes) => match &mut self.0 {
                Some(InputTextureState::Nv12(input)) => {
                    input.upload(ctx, planes, frame.resolution);
                }

                state => {
                    let mut input = NV12Input::new_uploadable(ctx, frame.resolution);
                    input.upload(ctx, planes, frame.resolution);
                    *state = Some(InputTextureState::Nv12(input));
                }
            },
            FrameData::InterleavedUyvy422(data) => {
                match &mut self.0 {
                    Some(InputTextureState::InterleavedUyvy422(input)) => {
                        input.upload(ctx, &data, frame.resolution);
                    }
                    state => {
                        let mut input = InterleavedUyvy422Input::new(ctx);
                        input.upload(ctx, &data, frame.resolution);
                        *state = Some(InputTextureState::InterleavedUyvy422(input));
                    }
                };
            }
            FrameData::InterleavedYuyv422(data) => match &mut self.0 {
                Some(InputTextureState::InterleavedYuyv422(input)) => {
                    input.upload(ctx, &data, frame.resolution);
                }
                state => {
                    let mut input = InterleavedYuyv422Input::new(ctx);
                    input.upload(ctx, &data, frame.resolution);
                    *state = Some(InputTextureState::InterleavedYuyv422(input));
                }
            },
            FrameData::Rgba8UnormWgpuTexture(texture) => {
                match &mut self.0 {
                    Some(InputTextureState::Rgba8Unorm(input)) => {
                        input.update(texture);
                    }
                    state => {
                        *state = Some(InputTextureState::Rgba8Unorm(RgbaTextureInput::new(
                            texture,
                        )));
                    }
                };
            }
            FrameData::Nv12WgpuTexture(texture) => {
                match &mut self.0 {
                    Some(InputTextureState::Nv12(input)) => {
                        input.update(ctx, texture).unwrap();
                    }
                    state => {
                        *state = Some(InputTextureState::Nv12(
                            NV12Input::new_from_texture(ctx, texture).unwrap(),
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
                    InputTextureState::PlanarYuv(state) => state.convert(ctx, dst_state),
                    InputTextureState::InterleavedUyvy422(state) => state.convert(ctx, dst_state),
                    InputTextureState::InterleavedYuyv422(state) => state.convert(ctx, dst_state),
                    InputTextureState::Rgba8Unorm(state) => state.convert(ctx, dst_state),
                    InputTextureState::Nv12(state) => state.convert(ctx, dst_state),
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
