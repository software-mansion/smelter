use std::collections::HashMap;

use crate::scene::{self, ComponentId, OutputNode};
use crate::{error::UpdateSceneError, wgpu::WgpuErrorScope};
use crate::{InputId, OutputFrameFormat, OutputId};

use super::input_texture::InputTexture;
use super::node::NewRenderNodeCtx;
use super::node_texture::NodeTexture;
use super::output_texture::OutputTexture;
use super::{node::RenderNode, RenderCtx};

pub(super) struct RenderGraph {
    pub(super) outputs: HashMap<OutputId, OutputRenderTree>,
    pub(super) inputs: HashMap<InputId, (NodeTexture, InputTexture)>,
}

pub(super) struct OutputRenderTree {
    pub(super) root: RenderNode,
    pub(super) output_texture: OutputTexture,
}

impl RenderGraph {
    pub fn empty() -> Self {
        Self {
            outputs: HashMap::new(),
            inputs: HashMap::new(),
        }
    }

    pub(super) fn register_input(&mut self, input_id: InputId) {
        self.inputs
            .insert(input_id, (NodeTexture::new(), InputTexture::new()));
    }

    pub(super) fn unregister_input(&mut self, input_id: &InputId) {
        self.inputs.remove(input_id);
    }

    pub(super) fn unregister_output(&mut self, output_id: &OutputId) {
        self.outputs.remove(output_id);
    }

    pub(super) fn update(
        &mut self,
        ctx: &RenderCtx,
        output: OutputNode,
        output_format: OutputFrameFormat,
    ) -> Result<(), UpdateSceneError> {
        // TODO: If we want nodes to be stateful we could try reusing nodes instead
        //       of recreating them on every scene update
        let scope = WgpuErrorScope::push(&ctx.wgpu_ctx.device);

        let mut previous_nodes = HashMap::new();
        if let Some(root_node) = self.outputs.get(&output.output_id) {
            gather_nodes_with_id(&root_node.root, &mut previous_nodes);
        }

        let output_tree = OutputRenderTree {
            root: Self::create_node(
                &NewRenderNodeCtx {
                    render_ctx: ctx,
                    previous_nodes: &previous_nodes,
                },
                output.node,
            )?,
            output_texture: OutputTexture::new(ctx.wgpu_ctx, output.resolution, output_format),
        };

        scope.pop(&ctx.wgpu_ctx.device)?;

        self.outputs.insert(output.output_id, output_tree);

        Ok(())
    }

    fn create_node(
        ctx: &NewRenderNodeCtx,
        node: scene::Node,
    ) -> Result<RenderNode, UpdateSceneError> {
        let children: Vec<RenderNode> = node
            .children
            .into_iter()
            .map(|node| Self::create_node(ctx, node))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RenderNode::new(ctx, node.id, node.params, children))
    }
}

fn gather_nodes_with_id<'a>(
    node: &'a RenderNode,
    nodes: &mut HashMap<ComponentId, &'a RenderNode>,
) {
    if let Some(id) = node.id.clone() {
        nodes.insert(id, node);
    }
    for node in node.children.iter() {
        gather_nodes_with_id(node, nodes);
    }
}
