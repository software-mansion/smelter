use std::mem;

use crate::{
    RenderingMode, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{RgbaLinearTexture, RgbaMultiViewTexture, RgbaSrgbTexture},
    },
};

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
                if state.resolution() == new_resolution {
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
}

impl Default for NodeTexture {
    fn default() -> Self {
        Self::new()
    }
}

pub enum NodeTextureState {
    GpuOptimized {
        texture: RgbaMultiViewTexture,
        linear_bind_group: wgpu::BindGroup,
        #[allow(dead_code)]
        srgb_bind_group: wgpu::BindGroup,
    },
    CpuOptimized {
        texture: RgbaLinearTexture,
        linear_bind_group: wgpu::BindGroup,
    },
    WebGl {
        texture: RgbaSrgbTexture,
        srgb_bind_group: wgpu::BindGroup,
    },
}

impl NodeTextureState {
    fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        match ctx.mode {
            RenderingMode::GpuOptimized => {
                let texture = RgbaMultiViewTexture::new(ctx, resolution);
                NodeTextureState::GpuOptimized {
                    linear_bind_group: texture.new_linear_bind_group(ctx),
                    srgb_bind_group: texture.new_srgb_bind_group(ctx),
                    texture,
                }
            }
            RenderingMode::CpuOptimized => {
                let texture = RgbaLinearTexture::new(ctx, resolution);
                NodeTextureState::CpuOptimized {
                    linear_bind_group: texture.new_bind_group(ctx),
                    texture,
                }
            }
            RenderingMode::WebGl => {
                let texture = RgbaSrgbTexture::new(ctx, resolution);
                NodeTextureState::WebGl {
                    srgb_bind_group: texture.new_bind_group(ctx),
                    texture,
                }
            }
        }
    }

    // bind group used to write to output texture
    pub fn output_texture_bind_group(&self) -> &wgpu::BindGroup {
        match &self {
            NodeTextureState::GpuOptimized {
                linear_bind_group, ..
            } => linear_bind_group,
            NodeTextureState::CpuOptimized {
                linear_bind_group, ..
            } => linear_bind_group,
            NodeTextureState::WebGl {
                srgb_bind_group, ..
            } => srgb_bind_group,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        match &self {
            NodeTextureState::GpuOptimized { texture, .. } => texture.srgb_view(),
            NodeTextureState::CpuOptimized { texture, .. } => texture.view(),
            NodeTextureState::WebGl { texture, .. } => texture.view(),
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        match &self {
            NodeTextureState::GpuOptimized { texture, .. } => texture.texture(),
            NodeTextureState::CpuOptimized { texture, .. } => texture.texture(),
            NodeTextureState::WebGl { texture, .. } => texture.texture(),
        }
    }

    pub fn resolution(&self) -> Resolution {
        self.texture().size().into()
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        match self {
            NodeTextureState::GpuOptimized { texture, .. } => texture.upload(ctx, data),
            NodeTextureState::CpuOptimized { texture, .. } => texture.upload(ctx, data),
            NodeTextureState::WebGl { texture, .. } => texture.upload(ctx, data),
        }
    }
}

/// Type that behaves like Option, but when is set to None
/// it keeps ownership of the value it had before.
#[derive(Default)]
enum OptionalState<State> {
    #[default]
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
            OptionalState::Some(state) => Some(state),
        }
    }

    fn replace(&mut self, replacement: Self) -> Self {
        mem::replace(self, replacement)
    }
}
