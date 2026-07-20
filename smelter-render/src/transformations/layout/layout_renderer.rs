use std::sync::Arc;

use crate::{
    RenderingMode,
    wgpu::{WgpuCtx, common_pipeline::CreateShaderError},
};

use super::{resampler::ResamplerShader, shader::LayoutShader};

pub struct LayoutRenderer {
    pub(super) shader: Arc<LayoutShader>,
    /// `None` in CPU-optimized rendering, which scales bilinearly instead.
    pub(super) resampler: Option<Arc<ResamplerShader>>,
}

impl LayoutRenderer {
    pub fn new(
        wgpu_ctx: &Arc<WgpuCtx>,
        max_layouts_count: usize,
    ) -> Result<Self, CreateShaderError> {
        let shader = Arc::new(LayoutShader::new(wgpu_ctx, max_layouts_count)?);
        let resampler = match wgpu_ctx.mode {
            RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                Some(Arc::new(ResamplerShader::new(wgpu_ctx)?))
            }
            RenderingMode::CpuOptimized => None,
        };
        Ok(Self { shader, resampler })
    }
}
