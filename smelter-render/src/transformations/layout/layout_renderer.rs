use std::sync::Arc;

use crate::wgpu::{common_pipeline::CreateShaderError, WgpuCtx};

use super::shader::LayoutShader;

pub struct LayoutRenderer(pub(super) Arc<LayoutShader>);

impl LayoutRenderer {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let shader = Arc::new(LayoutShader::new(wgpu_ctx)?);
        Ok(Self(shader))
    }
}
