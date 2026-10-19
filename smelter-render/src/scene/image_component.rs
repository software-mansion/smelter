use std::{sync::Arc, time::Duration};

use crate::{Resolution, scene::Size, transformations::image::Image};

use super::{
    ComponentId, ImageComponent, IntermediateNode, SceneError, StatefulComponent,
    scene_state::BuildStateTreeCtx,
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

    pub(super) fn image_render_params(self) -> ImageRenderParams {
        ImageRenderParams {
            image: self.image,
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

        let resolution = image_resolution(self.width, self.height, image.resolution());

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

/// Resolves the image size, filling in a missing `width` or `height` from the original aspect
/// ratio.
fn image_resolution(width: Option<f32>, height: Option<f32>, original: Resolution) -> Resolution {
    let original_aspect_ratio = original.width as f32 / original.height as f32;

    let (width, height) = match (width, height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, width / original_aspect_ratio),
        (None, Some(height)) => (height * original_aspect_ratio, height),
        (None, None) => return original,
    };
    Resolution {
        width: width.round() as usize,
        height: height.round() as usize,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_image_resolution() {
        let landscape = Resolution {
            width: 1920,
            height: 1080,
        };
        let portrait = Resolution {
            width: 1080,
            height: 1920,
        };
        let resolution = |width, height| Resolution { width, height };

        assert_eq!(image_resolution(None, None, landscape), landscape);
        assert_eq!(
            image_resolution(Some(100.0), Some(50.0), landscape),
            resolution(100, 50)
        );
        assert_eq!(
            image_resolution(Some(960.0), None, landscape),
            resolution(960, 540)
        );
        assert_eq!(
            image_resolution(None, Some(540.0), landscape),
            resolution(960, 540)
        );
        assert_eq!(
            image_resolution(Some(540.0), None, portrait),
            resolution(540, 960)
        );
        assert_eq!(
            image_resolution(None, Some(960.0), portrait),
            resolution(540, 960)
        );
    }
}
