use std::sync::Arc;

use crate::transformations::shader::Shader;

use super::{
    scene_state::BuildStateTreeCtx, Component, ComponentId, IntermediateNode, SceneError,
    ShaderComponent, ShaderParam, Size, StatefulComponent,
};

#[derive(Debug, Clone)]
pub(super) struct StatefulShaderComponent {
    pub(super) component: ShaderComponentParams,
    pub(super) children: Vec<StatefulComponent>,
    pub(super) shader: Arc<Shader>,
}

#[derive(Debug, Clone)]
pub(crate) struct ShaderComponentParams {
    pub(crate) id: Option<ComponentId>,
    pub(crate) shader_param: Option<ShaderParam>,
    pub(crate) size: Size,
}

impl StatefulShaderComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        let children = self
            .children
            .iter()
            .map(StatefulComponent::intermediate_node)
            .collect();

        IntermediateNode::Shader {
            shader: self.clone(),
            children,
        }
    }
}

impl ShaderComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let shader = ctx
            .renderers
            .shaders
            .get(&self.shader_id)
            .ok_or_else(|| SceneError::ShaderNotFound(self.shader_id.clone()))?;
        if let Some(params) = &self.shader_param {
            shader.validate_params(params).map_err(|err| {
                SceneError::ShaderNodeParametersValidationError(err, self.shader_id.clone())
            })?
        }

        let children = self
            .children
            .into_iter()
            .map(|c| Component::stateful_component(c, ctx))
            .collect::<Result<_, _>>()?;
        Ok(StatefulComponent::Shader(StatefulShaderComponent {
            component: ShaderComponentParams {
                id: self.id,
                shader_param: self.shader_param,
                size: self.size,
            },
            shader,
            children,
        }))
    }
}
