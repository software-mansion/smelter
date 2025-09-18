use std::sync::Arc;

use crate::transformations::web_renderer::WebRenderer;

use super::{
    scene_state::BuildStateTreeCtx, Component, ComponentId, IntermediateNode, SceneError, Size,
    StatefulComponent, WebViewComponent,
};

#[derive(Debug, Clone)]
pub(super) struct StatefulWebViewComponent {
    pub(super) id: Option<ComponentId>,
    pub(super) children: Vec<StatefulComponent>,
    pub(super) children_ids: Vec<ComponentId>,
    pub(super) instance: Arc<WebRenderer>,
}

impl StatefulWebViewComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.id.as_ref()
    }

    pub(super) fn size(&self) -> Size {
        self.instance.resolution().into()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        let children = self
            .children
            .iter()
            .map(StatefulComponent::intermediate_node)
            .collect();

        IntermediateNode::WebView {
            web: self.clone(),
            children,
        }
    }
}

impl WebViewComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let instance = ctx
            .renderers
            .web_renderers
            .get(&self.instance_id)
            .ok_or_else(|| SceneError::WebRendererNotFound(self.instance_id.clone()))?;

        let children = self
            .children
            .into_iter()
            .map(|c| Component::stateful_component(c, ctx))
            .collect::<Result<Vec<_>, _>>()?;
        let children_ids = children
            .iter()
            .map(|c| {
                c.component_id()
                    .cloned()
                    .ok_or(SceneError::WebViewChildWithoutId(self.instance_id.clone()))
            })
            .collect::<Result<_, _>>()?;
        Ok(StatefulComponent::WebView(StatefulWebViewComponent {
            id: self.id,
            instance,
            children,
            children_ids,
        }))
    }
}
