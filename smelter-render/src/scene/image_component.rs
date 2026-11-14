use std::{sync::Arc, time::Duration};

use crate::{
    Resolution,
    scene::Size,
    state::{RenderCtx, node_texture::NodeTexture},
    transformations::image::{Image, ImageNode},
};

use super::{
    ComponentId, ImageComponent, SceneError, StatefulComponent,
    scene_state::{BuildStateTreeCtx, PrevComponent},
};

#[derive(Debug)]
pub(crate) struct ImageRenderParams {
    pub(crate) image: Image,
    pub(crate) start_pts: Duration,
    pub(crate) resolution: Resolution,
}

pub(super) struct StatefulImageComponent {
    component: ImageComponent,
    image: Image,
    start_pts: Duration,
    resolution: Resolution,
    node: ImageNode,
    pub(super) output: NodeTexture,
}

impl StatefulImageComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    pub(super) fn size(&self) -> Size {
        Size {
            width: self.resolution.width as f32,
            height: self.resolution.height as f32,
        }
    }

    pub(super) fn render(&mut self, ctx: &mut RenderCtx, pts: Duration) {
        self.node.render(ctx, &mut self.output, pts);
    }
}

impl ImageComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let image = ctx
            .render_ctx
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
                PrevComponent::Image(image) => Some(*image),
                _ => None,
            });

        let prev_image = prev_state.map(|s| &s.image);
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

        // Unchanged image keeps its state, e.g. animation continues instead of starting over.
        let start_pts = match prev_state {
            Some(state) if self == state.component && are_images_matching => state.start_pts,
            _ => ctx.last_render_pts,
        };

        let node = ImageNode::new(
            ctx.render_ctx.wgpu_ctx,
            ImageRenderParams {
                image: image.clone(),
                start_pts,
                resolution,
            },
        );
        Ok(StatefulComponent::Image(StatefulImageComponent {
            component: self,
            image,
            start_pts,
            resolution,
            node,
            output: NodeTexture::new(),
        }))
    }
}
