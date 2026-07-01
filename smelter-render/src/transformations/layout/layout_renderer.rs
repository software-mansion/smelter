use std::sync::Arc;

use crate::wgpu::{WgpuCtx, common_pipeline::CreateShaderError};

use super::{lanczos_horizontal::LanczosHorizontalShader, shader::LayoutShader};

pub struct LayoutRenderer {
    pub(super) shader: Arc<LayoutShader>,
    pub(super) lanczos_horizontal: Arc<LanczosHorizontalShader>,
}

impl LayoutRenderer {
    pub fn new(wgpu_ctx: &Arc<WgpuCtx>) -> Result<Self, CreateShaderError> {
        let shader = Arc::new(LayoutShader::new(wgpu_ctx)?);
        let lanczos_horizontal = Arc::new(LanczosHorizontalShader::new(wgpu_ctx)?);
        Ok(Self { shader, lanczos_horizontal })
    }
}
