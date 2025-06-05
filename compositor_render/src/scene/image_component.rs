use std::{sync::Arc, time::Duration};

use crate::{scene::Size, transformations::image::Image};

use super::{
    scene_state::BuildStateTreeCtx, ComponentId, ImageComponent, IntermediateNode, SceneError, StatefulComponent,
};

#[derive(Debug)]
pub(crate) struct ImageRenderParams {
     pub(crate) image: Image,
     pub(crate) start_pts: Duration,
     pub(crate) width: Option<f32>,
     pub(crate) height: Option<f32>
}

#[derive(Debug, Clone)]
pub(super) struct StatefulImageComponent {
    pub(super) component: ImageComponent,
    pub(super) image: Image,
    pub(super) start_pts: Duration,
    // pub(crate) width: Option<usize>,
    // pub(crate) height: Option<usize>
}

impl StatefulImageComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn width(&self) -> f32 {
        self.component.width.unwrap() as f32 // TODO
    }
    pub(super) fn height(&self) -> f32 {
        self.component.height.unwrap() as f32 // TODO
    }
    pub(super) fn size(&self) -> Size {
        Size {width: self.width(),  height: self.height()} // TODO
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

        let prev_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                StatefulComponent::Image(image) => Some(image),
                _ => None,
            });

        let prev_image = prev_state.as_ref().map(|s| &s.image);
        let are_images_matching = match (prev_image, &image) {
            (Some(Image::Bitmap(previous)), Image::Bitmap(current)) => {
                Arc::ptr_eq(previous, current)
            }
            (Some(Image::Animated(previous)), Image::Animated(current)) => {
                Arc::ptr_eq(previous, current)
            }
            (Some(Image::Svg(previous)), Image::Svg(current)) => Arc::ptr_eq(previous, current),
            (_, _) => false,
        };

        let component = match prev_state {
            Some(state) if self == state.component && are_images_matching => state.clone(),
            _ => StatefulImageComponent {
                component: self,
                image,
                start_pts: ctx.last_render_pts,
            },
        };

        Ok(StatefulComponent::Image(component))
    }
}
