use std::{sync::Arc, time::Duration};

use crate::{Resolution, scene::Size, transformations::image::Image};

use super::{
    ComponentId, ImageComponent, SceneError, StatefulComponent, scene_state::BuildStateTreeCtx,
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

    pub(super) fn image_render_params(&self) -> ImageRenderParams {
        ImageRenderParams {
            image: self.image.clone(),
            start_pts: self.start_pts,
            resolution: self.resolution,
        }
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

        let original_size = image.size();

        let size = match (self.width, self.height) {
            (Some(width), Some(height)) => Size { width, height },
            (Some(width), None) => {
                let height = if original_size.width > 0.0 {
                    width * original_size.height / original_size.width
                } else {
                    0.0
                };
                Size { width, height }
            }
            (None, Some(height)) => {
                let width = if original_size.height > 0.0 {
                    height * original_size.width / original_size.height
                } else {
                    0.0
                };
                Size { width, height }
            }
            (None, None) => original_size,
        };
        let resolution = Resolution {
            width: size.width.round() as usize,
            height: size.height.round() as usize,
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
