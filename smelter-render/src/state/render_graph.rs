use std::collections::HashMap;

use crate::scene;
use crate::{InputId, OutputFrameFormat, OutputId, Resolution};
use crate::{error::UpdateSceneError, wgpu::WgpuErrorScope};

use super::input_texture::InputTexture;
use super::node_texture::NodeTexture;
use super::output_texture::OutputTexture;
use super::{RenderCtx, node::RenderNode};

pub(super) struct RenderGraph {
    /// Root nodes keyed by the output that defines the scene.
    pub(super) render_trees: HashMap<OutputId, RenderNode>,
    pub(super) outputs: HashMap<OutputId, OutputRenderer>,
    pub(super) inputs: HashMap<InputId, (NodeTexture, InputTexture)>,
}

pub(super) struct OutputRenderer {
    /// Own id, or id of the primary output for linked outputs.
    pub(super) render_tree_id: OutputId,
    pub(super) output_texture: OutputTexture,
}

pub(super) enum OutputSpec {
    RenderTree(scene::Node),
    LinkedOutput(OutputId),
}

impl RenderGraph {
    pub fn empty() -> Self {
        Self {
            render_trees: HashMap::new(),
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
        self.render_trees.remove(output_id);
        self.outputs.remove(output_id);
    }

    pub(super) fn update_output(
        &mut self,
        ctx: &RenderCtx,
        output_id: OutputId,
        resolution: Resolution,
        output_format: OutputFrameFormat,
        source: OutputSpec,
    ) -> Result<(), UpdateSceneError> {
        // TODO: If we want nodes to be stateful we could try reusing nodes instead
        //       of recreating them on every scene update
        let scope = WgpuErrorScope::push(&ctx.wgpu_ctx.device);

        let output_texture = OutputTexture::new(ctx.wgpu_ctx, resolution, output_format);
        let (root, render_tree_id) = match source {
            OutputSpec::RenderTree(node) => {
                (Some(Self::create_node(ctx, node)?), output_id.clone())
            }
            OutputSpec::LinkedOutput(primary_output_id) => (None, primary_output_id),
        };

        scope.pop()?;

        match root {
            Some(root) => self.render_trees.insert(output_id.clone(), root),
            None => self.render_trees.remove(&output_id),
        };
        self.outputs.insert(
            output_id,
            OutputRenderer {
                render_tree_id,
                output_texture,
            },
        );

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
