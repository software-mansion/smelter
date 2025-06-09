use std::{sync::Arc, time::Duration};

use crate::{scene::Size, transformations::image::Image, Resolution};

use super::{
    scene_state::BuildStateTreeCtx, ComponentId, ImageComponent, IntermediateNode, SceneError,
    StatefulComponent,
};

#[derive(Debug)]
pub(crate) struct ImageRenderParams {
    pub(crate) image: Image,
    pub(crate) start_pts: Duration,
    pub(crate) resolution: Resolution,
}

#[derive(Debug, Clone)]
pub(super) struct StatefulImageComponent {
    pub(super) component: ImageComponent,
    pub(super) image: Image,
    pub(super) start_pts: Duration,
    pub(super) resolution: Resolution,
}

impl StatefulImageComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn width(&self) -> f32 {
        self.resolution.width as f32
    }
    pub(super) fn height(&self) -> f32 {
        self.resolution.height as f32
    }
    pub(super) fn size(&self) -> Size {
        Size {
            width: self.width(),
            height: self.height(),
        }
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

        let original_aspect_ratio =
            image.resolution().width as f32 / image.resolution().height as f32;

        let resolution = match (self.width, self.height) {
            (Some(width), Some(height)) => Resolution { width, height },
            (Some(width), None) => {
                let height = (width as f32 / original_aspect_ratio).round() as usize;
                Resolution { width, height }
            }
            (None, Some(height)) => {
                let width = (height as f32 * original_aspect_ratio).round() as usize;
                Resolution { width, height }
            }
            (None, None) => image.resolution(),
        };

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
                resolution,
            },
        };

        Ok(StatefulComponent::Image(component))
    }
}
