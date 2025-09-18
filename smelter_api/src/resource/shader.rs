use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_render::shader;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShaderSpec {
    /// Shader source code. [Learn more.](../../concept/shaders)
    pub source: String,
}

impl TryFrom<ShaderSpec> for smelter_render::RendererSpec {
    type Error = TypeError;

    fn try_from(spec: ShaderSpec) -> Result<Self, Self::Error> {
        let spec = shader::ShaderSpec {
            source: spec.source.into(),
        };
        Ok(Self::Shader(spec))
    }
}
