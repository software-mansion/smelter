use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::vec;

use crate::scene::{self, image_component::ImageRenderParams, ComponentId, ShaderComponentParams};
use crate::transformations::layout::LayoutNode;
use crate::transformations::shader::node::ShaderNode;
use crate::transformations::shader::Shader;
use crate::InputId;

use crate::transformations::text_renderer::TextRenderParams;
use crate::transformations::web_renderer::WebRenderer;
use crate::transformations::{
    image::ImageNode, text_renderer::TextRendererNode, web_renderer::WebRendererNode,
};

use super::input_texture::InputTexture;
use super::node_texture::NodeTexture;
use super::RenderCtx;

pub(super) enum InnerRenderNode {
    Shader(ShaderNode),
    Web(WebRendererNode),
    Text(TextRendererNode),
    Image(ImageNode),
    Layout(LayoutNode),
    InputStreamRef(InputId),
}

impl InnerRenderNode {
    pub fn render(
        &mut self,
        ctx: &mut RenderCtx,
        sources: &[&NodeTexture],
        target: &mut NodeTexture,
        pts: Duration,
    ) {
        match self {
            InnerRenderNode::Shader(ref shader) => {
                shader.render(ctx.wgpu_ctx, sources, target, pts);
            }
            InnerRenderNode::Web(renderer) => renderer.render(ctx, sources, target),
            InnerRenderNode::Text(renderer) => {
                renderer.render(ctx, target);
            }
            InnerRenderNode::Image(node) => node.render(ctx, target, pts),
            InnerRenderNode::InputStreamRef(_) => {
                // Nothing to do, textures on input nodes should be populated
                // at the start of render loop
            }
            InnerRenderNode::Layout(node) => node.render(ctx, sources, target, pts),
        }
    }
}

pub(super) struct RenderNode {
    pub(super) output: NodeTexture,
    pub(super) renderer: InnerRenderNode,
    pub(super) children: Vec<RenderNode>,
}

impl RenderNode {
    pub(super) fn new(
        ctx: &RenderCtx,
        params: scene::NodeParams,
        children: Vec<RenderNode>,
    ) -> Self {
        match params {
            scene::NodeParams::InputStream(id) => Self {
                output: NodeTexture::new(),
                renderer: InnerRenderNode::InputStreamRef(id),
                children,
            },
            scene::NodeParams::Shader(shader_params, shader) => {
                Self::new_shader_node(ctx, children, shader_params, shader)
            }
            scene::NodeParams::Web(children_ids, web_renderer) => {
                Self::new_web_renderer_node(ctx, children, children_ids, web_renderer)
            }
            scene::NodeParams::Image(image_params) => Self::new_image_node(ctx, image_params),
            scene::NodeParams::Text(text_params) => Self::new_text_node(ctx, text_params),
            scene::NodeParams::Layout(layout_provider) => {
                Self::new_layout_node(ctx, children, layout_provider)
            }
        }
    }

    /// Helper to access real texture backing up specific node. For all nodes this is
    /// equivalent of accessing output field, but in case of InputStreamRef `output` field
    /// is just a stub that does not do anything.
    pub(super) fn output_texture<'a>(
        &'a self,
        inputs: &'a HashMap<InputId, (NodeTexture, InputTexture)>,
    ) -> &'a NodeTexture {
        match &self.renderer {
            InnerRenderNode::InputStreamRef(id) => inputs
                .get(id)
                .map(|(node_texture, _)| node_texture)
                .unwrap_or(&self.output),
            _non_input_stream => &self.output,
        }
    }

    fn new_shader_node(
        ctx: &RenderCtx,
        children: Vec<RenderNode>,
        shader_params: ShaderComponentParams,
        shader: Arc<Shader>,
    ) -> Self {
        let node = InnerRenderNode::Shader(ShaderNode::new(
            ctx,
            shader,
            &shader_params.shader_param,
            &shader_params.size.into(),
        ));
        let mut output = NodeTexture::new();
        output.ensure_size(ctx.wgpu_ctx, shader_params.size.into());

        Self {
            renderer: node,
            output,
            children,
        }
    }

    pub(super) fn new_web_renderer_node(
        ctx: &RenderCtx,
        children: Vec<RenderNode>,
        children_ids: Vec<ComponentId>,
        web_renderer: Arc<WebRenderer>,
    ) -> Self {
        let resolution = web_renderer.resolution();
        let node = InnerRenderNode::Web(WebRendererNode::new(children_ids, web_renderer));
        let mut output = NodeTexture::new();
        output.ensure_size(ctx.wgpu_ctx, resolution);

        Self {
            renderer: node,
            output,
            children,
        }
    }

    pub(super) fn new_image_node(ctx: &RenderCtx, image: ImageRenderParams) -> Self {
        let node = InnerRenderNode::Image(ImageNode::new(ctx.wgpu_ctx, image));
        let output = NodeTexture::new();

        Self {
            renderer: node,
            output,
            children: vec![],
        }
    }

    pub(super) fn new_text_node(ctx: &RenderCtx, params: TextRenderParams) -> Self {
        let node = InnerRenderNode::Text(TextRendererNode::new(ctx, params));
        let output = NodeTexture::new();

        Self {
            renderer: node,
            output,
            children: vec![],
        }
    }

    pub(super) fn new_layout_node(
        ctx: &RenderCtx,
        children: Vec<RenderNode>,
        provider: scene::LayoutNode,
    ) -> Self {
        let node = InnerRenderNode::Layout(LayoutNode::new(ctx, Box::new(provider)));
        let output = NodeTexture::new();

        Self {
            renderer: node,
            output,
            children,
        }
    }
}
