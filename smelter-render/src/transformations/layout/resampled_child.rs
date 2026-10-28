use crate::{
    Resolution,
    state::node_texture::{NodeTexture, NodeTextureState},
    wgpu::{
        WgpuCtx,
        resampler::{Crop, Lanczos3TextureCache},
    },
};

/// Child node resampled to its exact on-screen size.
#[derive(Default)]
pub(super) struct ResampledChild {
    texture_cache: Lanczos3TextureCache,
    output: NodeTexture,
}

impl ResampledChild {
    /// Crop left for the layout shader: the resampler consumes the entire original crop and emits
    /// the exact `dst`-sized rect.
    pub(super) fn output_crop(dst: Resolution) -> Crop {
        Crop {
            top: 0.0,
            left: 0.0,
            width: dst.width as f32,
            height: dst.height as f32,
        }
    }

    pub(super) fn output_state(&self) -> Option<&NodeTextureState> {
        self.output.state()
    }

    pub(super) fn render(
        &mut self,
        wgpu_ctx: &WgpuCtx,
        source: &NodeTextureState,
        crop: Crop,
        dst: Resolution,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let target = self.output.ensure_size(wgpu_ctx, dst);
        let resampler = match target {
            NodeTextureState::CpuOptimized { .. } => &wgpu_ctx.resamplers.lanczos3_linear,
            NodeTextureState::GpuOptimized { .. } | NodeTextureState::WebGl { .. } => {
                &wgpu_ctx.resamplers.lanczos3_srgb
            }
        };
        resampler.resample(
            wgpu_ctx,
            source.view(),
            crop,
            target.view(),
            &mut self.texture_cache,
            encoder,
        );
    }
}
