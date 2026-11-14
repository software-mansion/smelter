use std::collections::HashMap;

use crate::{
    InputId, Resolution,
    state::{InputTextures, node_texture::NodeTexture},
};

use super::{
    ComponentId, InputStreamComponent, SceneError, Size, StatefulComponent,
    scene_state::{BuildStateTreeCtx, LayoutCtx},
};

pub(super) struct StatefulInputStreamComponent {
    pub(super) component: InputStreamComponent,
    /// Resolution of the input frame. Zero if the input does not have a frame.
    pub(super) size: Size,
    /// Empty texture returned if the input is not registered.
    empty_texture: NodeTexture,
}

impl StatefulInputStreamComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn layout(&mut self, ctx: &LayoutCtx) {
        self.size = frame_size(&self.component.input_id, ctx.input_resolutions);
    }

    pub(super) fn texture<'a>(&'a self, inputs: &'a InputTextures) -> &'a NodeTexture {
        inputs
            .get(&self.component.input_id)
            .map(|(node_texture, _)| node_texture)
            .unwrap_or(&self.empty_texture)
    }
}

impl InputStreamComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        Ok(StatefulComponent::InputStream(
            StatefulInputStreamComponent {
                size: frame_size(&self.input_id, ctx.input_resolutions),
                component: self,
                empty_texture: NodeTexture::new(),
            },
        ))
    }
}

fn frame_size(input_id: &InputId, input_resolutions: &HashMap<InputId, Resolution>) -> Size {
    input_resolutions
        .get(input_id)
        .copied()
        .unwrap_or(Resolution {
            width: 0,
            height: 0,
        })
        .into()
}
