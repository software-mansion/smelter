use std::{sync::Arc, time::Duration};

use crate::{
    state::{InputTextures, RenderCtx, node_texture::NodeTexture},
    transformations::web_renderer::{WebRenderer, WebRendererNode},
};

use super::{
    Component, ComponentId, SceneError, Size, StatefulComponent, WebViewComponent,
    scene_state::BuildStateTreeCtx,
};

pub(super) struct StatefulWebViewComponent {
    id: Option<ComponentId>,
    pub(super) children: Vec<StatefulComponent>,
    instance: Arc<WebRenderer>,
    node: WebRendererNode,
    pub(super) output: NodeTexture,
}

impl StatefulWebViewComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.id.as_ref()
    }

    pub(super) fn size(&self) -> Size {
        self.instance.resolution().into()
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
        self.node.render(ctx, &sources, &mut self.output);
    }
}

impl WebViewComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let instance = ctx
            .render_ctx
            .renderers
            .web_renderers
            .get(&self.instance_id)
            .ok_or_else(|| SceneError::WebRendererNotFound(self.instance_id.clone()))?;

        let children = self
            .children
            .into_iter()
            .map(|c| Component::stateful_component(c, ctx, None))
            .collect::<Result<Vec<_>, _>>()?;
        let children_ids = children
            .iter()
            .map(|c| {
                c.component_id()
                    .cloned()
                    .ok_or(SceneError::WebViewChildWithoutId(self.instance_id.clone()))
            })
            .collect::<Result<_, _>>()?;

        let node = WebRendererNode::new(children_ids, instance.clone());
        let mut output = NodeTexture::new();
        output.ensure_size(ctx.render_ctx.wgpu_ctx, instance.resolution());

        Ok(StatefulComponent::WebView(StatefulWebViewComponent {
            id: self.id,
            children,
            instance,
            node,
            output,
        }))
    }
}
