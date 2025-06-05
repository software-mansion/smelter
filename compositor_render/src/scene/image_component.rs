use crate::{transformations::image::Image, Resolution};

use super::{
    scene_state::BuildStateTreeCtx, ComponentId, ImageComponent, IntermediateNode, SceneError,
    Size, StatefulComponent,
};

#[derive(Debug, Clone)]
pub(super) struct StatefulImageComponent {
    pub(super) component: ImageComponent,
    pub(super) image: Image,
    // pub(super) resolution: Option<Resolution>
}

impl StatefulImageComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn size(&self) -> Size {
        self.component.resolution.unwrap_or(self.image.resolution()).into()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        IntermediateNode::Image(self.clone())
    }
}

impl ImageComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let image = ctx
            .renderers
            .images
            .get(&self.image_id)
            .ok_or_else(|| SceneError::ImageNotFound(self.image_id.clone()))?;
        Ok(StatefulComponent::Image(StatefulImageComponent {
            component: self,
            image,
            // resolution: self.resolution
        }))
    }
}
