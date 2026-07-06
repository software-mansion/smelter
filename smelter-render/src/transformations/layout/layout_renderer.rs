use std::sync::Arc;

use crate::wgpu::{WgpuCtx, common_pipeline::CreateShaderError};

use super::{resampler::ResamplerShader, shader::LayoutShader};

pub struct LayoutRenderer {
    pub(super) shader: Arc<LayoutShader>,
    pub(super) resampler: Arc<ResamplerShader>,
}

impl LayoutRenderer {
    pub fn new(
        wgpu_ctx: &Arc<WgpuCtx>,
        max_layouts_count: usize,
    ) -> Result<Self, CreateShaderError> {
        let shader = Arc::new(LayoutShader::new(wgpu_ctx, max_layouts_count)?);
        let resampler = Arc::new(ResamplerShader::new(wgpu_ctx)?);
        Ok(Self { shader, resampler })
    }
}
