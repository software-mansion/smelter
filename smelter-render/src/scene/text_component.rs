use crate::{
    Resolution,
    state::{RenderCtx, node_texture::NodeTexture},
    transformations::text_renderer::{TextRenderParams, TextRendererNode},
};

use super::{
    ComponentId, SceneError, Size, StatefulComponent, TextComponent, scene_state::BuildStateTreeCtx,
};

pub(super) struct StatefulTextComponent {
    id: Option<ComponentId>,
    resolution: Resolution,
    node: TextRendererNode,
    pub(super) output: NodeTexture,
}

impl StatefulTextComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.id.as_ref()
    }

    pub(super) fn size(&self) -> Size {
        self.resolution.into()
    }

    pub(super) fn render(&mut self, ctx: &mut RenderCtx) {
        self.node.render(ctx, &mut self.output);
    }
}

impl TextComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let (buffer, resolution) = ctx
            .render_ctx
            .text_renderer_ctx
            .layout_text((&self).into(), self.dimensions);
        let node = TextRendererNode::new(
            ctx.render_ctx,
            TextRenderParams {
                buffer,
                resolution,
                background_color: self.background_color,
            },
        );
        Ok(StatefulComponent::Text(StatefulTextComponent {
            id: self.id,
            resolution,
            node,
            output: NodeTexture::new(),
        }))
    }
}
