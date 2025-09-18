use std::collections::HashMap;

use crate::scene::{self, OutputNode};
use crate::{error::UpdateSceneError, wgpu::WgpuErrorScope};
use crate::{InputId, OutputFrameFormat, OutputId};

use super::input_texture::InputTexture;
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

        let output_tree = OutputRenderTree {
            root: Self::create_node(ctx, output.node)?,
            output_texture: OutputTexture::new(ctx.wgpu_ctx, output.resolution, output_format),
        };

        scope.pop(&ctx.wgpu_ctx.device)?;

        self.outputs.insert(output.output_id, output_tree);

        Ok(())
    }

    fn create_node(ctx: &RenderCtx, node: scene::Node) -> Result<RenderNode, UpdateSceneError> {
        let children: Vec<RenderNode> = node
            .children
            .into_iter()
            .map(|node| Self::create_node(ctx, node))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RenderNode::new(ctx, node.params, children))
    }
}
