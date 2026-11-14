use std::time::Duration;

use crate::{
    state::{InputTextures, RenderCtx, node_texture::NodeTexture},
    transformations::shader::node::ShaderNode,
};

use super::{
    Component, ComponentId, SceneError, ShaderComponent, Size, StatefulComponent,
    scene_state::BuildStateTreeCtx,
};

pub(super) struct StatefulShaderComponent {
    id: Option<ComponentId>,
    pub(super) size: Size,
    pub(super) children: Vec<StatefulComponent>,
    node: ShaderNode,
    pub(super) output: NodeTexture,
}

impl StatefulShaderComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.id.as_ref()
    }

    pub(super) fn render(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        for child in self.children.iter_mut() {
            child.render(ctx, inputs, pts);
        }
        let sources: Vec<_> = self
            .children
            .iter()
            .map(|child| child.texture(inputs))
            .collect();
        self.node.render(ctx, &sources, &mut self.output, pts);
    }
}

impl ShaderComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let shader = ctx
            .render_ctx
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
            .map(|c| Component::stateful_component(c, ctx, None))
            .collect::<Result<_, _>>()?;

        let resolution = self.size.into();
        let node = ShaderNode::new(ctx.render_ctx, shader, &self.shader_param, &resolution);
        let mut output = NodeTexture::new();
        output.ensure_size(ctx.render_ctx.wgpu_ctx, resolution);

        Ok(StatefulComponent::Shader(StatefulShaderComponent {
            id: self.id,
            size: self.size,
            children,
            node,
            output,
        }))
    }
}
