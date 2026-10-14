use std::collections::HashMap;

use crate::{InputId, OutputId};

use super::input_texture::InputTexture;
use super::node::RenderNode;
use super::node_texture::NodeTexture;
use super::output_texture::OutputTexture;

pub(super) struct RenderGraph {
    pub(super) outputs: HashMap<OutputId, OutputRenderTree>,
    pub(super) inputs: HashMap<InputId, (NodeTexture, InputTexture)>,
}

pub(super) struct OutputRenderTree {
    pub(super) root: Option<RenderNode>,
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

    /// Render nodes are created on the next render from the updated scene state.
    pub(super) fn update(&mut self, output_id: OutputId, output_texture: OutputTexture) {
        // TODO: If we want nodes to be stateful we could try reusing nodes instead
        //       of recreating them on every scene update
        let output_tree = OutputRenderTree {
            root: None,
            output_texture,
        };
        self.outputs.insert(output_id, output_tree);
    }
}
