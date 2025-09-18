use crate::transformations::text_renderer::TextRenderParams;

use super::{
    scene_state::BuildStateTreeCtx, ComponentId, IntermediateNode, SceneError, Size,
    StatefulComponent, TextComponent,
};

#[derive(Debug, Clone)]
pub(super) struct StatefulTextComponent {
    id: Option<ComponentId>,
    pub(super) params: TextRenderParams,
}

impl StatefulTextComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.id.as_ref()
    }

    pub(super) fn width(&self) -> f32 {
        self.params.resolution.width as f32
    }

    pub(super) fn height(&self) -> f32 {
        self.params.resolution.height as f32
    }

    pub(super) fn size(&self) -> Size {
        self.params.resolution.into()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        IntermediateNode::Text(self.clone())
    }
}

impl TextComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let (buffer, resolution) = ctx
            .text_renderer_ctx
            .layout_text((&self).into(), self.dimensions);
        Ok(StatefulComponent::Text(StatefulTextComponent {
            id: self.id,
            params: TextRenderParams {
                buffer,
                resolution,
                background_color: self.background_color,
            },
        }))
    }
}
