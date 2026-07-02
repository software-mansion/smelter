use std::sync::Arc;

use crate::wgpu::{WgpuCtx, common_pipeline::CreateShaderError};

use super::{LayoutLimits, shader::LayoutShader};

pub struct LayoutRenderer(pub(super) Arc<LayoutShader>);

impl LayoutRenderer {
    pub fn new(
        wgpu_ctx: &Arc<WgpuCtx>,
        max_layouts_count: usize,
    ) -> Result<Self, CreateShaderError> {
        let shader = Arc::new(LayoutShader::new(
            wgpu_ctx,
            LayoutLimits::new(max_layouts_count),
        )?);
        Ok(Self(shader))
    }
}
