use std::{iter, mem};

use crate::{scene::RGBAColor, Resolution};

use super::{
    BoxShadow, Crop, LayoutContent, Mask, NestedLayout, RenderLayout, RenderLayoutContent,
};

impl NestedLayout {
    pub(super) fn flatten(
        self,
        input_resolutions: &[Option<Resolution>],
        resolution: Resolution,
    ) -> Vec<RenderLayout> {
        let (shadow, layouts) = self.inner_flatten(0, vec![]);
        shadow
            .into_iter()
            .chain(layouts)
            .filter(|layout| Self::should_render(layout, input_resolutions, resolution))
            .map(NestedLayout::fix_final_render_layout)
            .collect()
    }

    fn inner_flatten(
        mut self,
        child_index_offset: usize,
        parent_masks: Vec<Mask>,
    ) -> (Vec<RenderLayout>, Vec<RenderLayout>) {
        let mut child_index_offset = child_index_offset;
        if let LayoutContent::ChildNode { index, size } = self.content {
            self.content = LayoutContent::ChildNode {
                index: index + child_index_offset,
                size,
            };
            child_index_offset += 1
        }
        let layout = self.render_layout(&parent_masks);
        // It is separated because box shadows of all siblings need to be rendered before
        // this layout and it's siblings
        let box_shadow_layouts = self
            .box_shadow
            .iter()
            .map(|shadow| self.box_shadow_layout(shadow, &parent_masks))
            .collect();

        let parent_masks = match &self.mask {
            Some(mask) => parent_masks
                .iter()
                .chain(iter::once(mask))
                .cloned()
                .collect(),
            None => parent_masks.clone(),
        };
        let parent_masks = self.child_parent_masks(&parent_masks);

        let (children_shadow, children_layouts): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.children)
                .into_iter()
                .map(|child| {
                    let child_nodes_count = child.child_nodes_count;
                    let (shadows, layouts) =
                        child.inner_flatten(child_index_offset, parent_masks.clone());
                    child_index_offset += child_nodes_count;
                    (shadows, layouts)
                })
                .unzip();
        let children_shadow = children_shadow
            .into_iter()
            .flatten()
            .map(|l| self.flatten_child(l))
            .collect();
        let children_layouts = children_layouts
            .into_iter()
            .flatten()
            .map(|l| self.flatten_child(l))
            .collect();

        (
            box_shadow_layouts,
            [vec![layout], children_shadow, children_layouts].concat(),
        )
    }

    // Final pass on each render layout, it applies following modifications:
    // - If border_width is between 0 and 1 set it to 1.
    // - Remove masks that don't do anything
    fn fix_final_render_layout(mut layout: RenderLayout) -> RenderLayout {
        fn filter_mask(layout: &RenderLayout, mask: Mask) -> Option<Mask> {
            let max_top_border = f32::max(mask.radius.top_left, mask.radius.top_right);
            let max_bottom_border = f32::max(mask.radius.bottom_left, mask.radius.bottom_right);
            let max_left_border = f32::max(mask.radius.top_left, mask.radius.bottom_left);
            let max_right_border = f32::max(mask.radius.top_right, mask.radius.bottom_right);
            let should_skip = mask.top + max_top_border <= layout.top
                && mask.left + max_left_border <= layout.left
                && mask.left + mask.width - max_right_border >= layout.left + layout.width
                && mask.top + mask.height - max_bottom_border >= layout.top + layout.height;
            match should_skip {
                true => None,
                false => Some(mask),
            }
        }
        match &mut layout.content {
            RenderLayoutContent::Color { border_width, .. }
            | RenderLayoutContent::ChildNode { border_width, .. } => {
                if *border_width < 1.0 {
                    *border_width = 0.0
                }
            }
            _ => (),
        };
        layout.masks = mem::take(&mut layout.masks)
            .into_iter()
            .filter_map(|mask| filter_mask(&layout, mask))
            .collect();
        layout
    }

    // Decides if layout will affect the output of the stream, if not this layout will not be
    // passed to the shader.
    // Layouts are in absolute units at this point.
    fn should_render(
        layout: &RenderLayout,
        input_resolutions: &[Option<Resolution>],
        resolution: Resolution,
    ) -> bool {
        if layout.width <= 0.0
            || layout.height <= 0.0
            || layout.top > resolution.height as f32
            || layout.left > resolution.width as f32
        {
            return false;
        }
        match &layout.content {
            RenderLayoutContent::Color {
                color: RGBAColor(_, _, _, 0),
                border_color: RGBAColor(_, _, _, border_alpha),
                border_width,
            } => *border_alpha != 0 || *border_width > 0.0,
            RenderLayoutContent::Color { .. } => true,
            RenderLayoutContent::ChildNode {
                crop,
                index,
                border_color: RGBAColor(_, _, _, _),
                border_width: _,
            } => {
                // TODO: handle a case when only border is visible (currently impossible)
                let size = input_resolutions.get(*index).copied().flatten();
                if let Some(size) = size {
                    if crop.left > size.width as f32 || crop.top > size.height as f32 {
                        return false;
                    }
                }
                if crop.top + crop.height < 0.0 || crop.left + crop.width < 0.0 {
                    return false;
                }
                true
            }
            RenderLayoutContent::BoxShadow {
                color: RGBAColor(_, _, _, 0),
                ..
            } => false,
            RenderLayoutContent::BoxShadow { .. } => true,
        }
    }

    // parent_masks - in self coordinates
    fn flatten_child(&self, child: RenderLayout) -> RenderLayout {
        // scale factor used if we need to scale something that can't be
        // scaled separately for horizontal and vertical direction.
        let unified_scale = f32::min(self.scale_x, self.scale_y);

        match &self.crop {
            None => RenderLayout {
                top: self.top + (child.top * self.scale_y),
                left: self.left + (child.left * self.scale_x),
                width: child.width * self.scale_x,
                height: child.height * self.scale_y,
                rotation_degrees: child.rotation_degrees + self.rotation_degrees, // TODO: not exactly correct
                content: match child.content {
                    RenderLayoutContent::Color {
                        color,
                        border_color,
                        border_width,
                    } => RenderLayoutContent::Color {
                        color,
                        border_color,
                        border_width: border_width * unified_scale,
                    },
                    RenderLayoutContent::ChildNode {
                        index,
                        border_color,
                        border_width,
                        crop,
                    } => RenderLayoutContent::ChildNode {
                        index,
                        border_color,
                        border_width: border_width * unified_scale,
                        crop,
                    },
                    RenderLayoutContent::BoxShadow { color, blur_radius } => {
                        RenderLayoutContent::BoxShadow {
                            color,
                            blur_radius: blur_radius * unified_scale,
                        }
                    }
                },
                // TODO: This will not work correctly for layouts that are not proportionally
                // scaled
                border_radius: child.border_radius * unified_scale,
                masks: self.parent_parent_masks(&child.masks),
            },
            Some(crop) => {
                // Below values are only correct if `crop` is in the same coordinate
                // system as self.top/self.left/self.width/self.height. This condition
                // will always be fulfilled as long NestedLayout with LayoutContent::ChildNode
                // does not have any child layouts.

                // Value in coordinates of `self` (relative to it's top-left corner). Represents
                // a position after cropping and translated back to (layout.top, layout.left).
                let cropped_top = f32::max(child.top - crop.top, 0.0);
                let cropped_left = f32::max(child.left - crop.left, 0.0);
                let cropped_bottom = f32::min(child.top + child.height - crop.top, crop.height);
                let cropped_right = f32::min(child.left + child.width - crop.left, crop.width);
                let cropped_width = cropped_right - cropped_left;
                let cropped_height = cropped_bottom - cropped_top;
                match child.content.clone() {
                    RenderLayoutContent::Color {
                        color,
                        border_color,
                        border_width,
                    } => {
                        RenderLayout {
                            top: self.top + (cropped_top * self.scale_y),
                            left: self.left + (cropped_left * self.scale_x),
                            width: cropped_width * self.scale_x,
                            height: cropped_height * self.scale_y,
                            rotation_degrees: child.rotation_degrees + self.rotation_degrees, // TODO: not exactly correct
                            content: RenderLayoutContent::Color {
                                color,
                                border_color,
                                border_width: border_width * unified_scale,
                            },
                            border_radius: child.border_radius * unified_scale,
                            masks: self.parent_parent_masks(&child.masks),
                        }
                    }
                    RenderLayoutContent::ChildNode {
                        index,
                        crop: child_crop,
                        border_color,
                        border_width,
                    } => {
                        // Calculate how much top/left coordinates changed when cropping. It represents
                        // how much was removed in layout coordinates. Ignore the change of a position that
                        // was a result of a translation after cropping.
                        let top_diff = f32::max(crop.top - child.top, 0.0);
                        let left_diff = f32::max(crop.left - child.left, 0.0);

                        // Factor to translate from `layout` coordinates to child node coord.
                        // The same factor holds for translations from `self.layout`.
                        let horizontal_scale_factor = child_crop.width / child.width;
                        let vertical_scale_factor = child_crop.height / child.height;

                        let crop = Crop {
                            top: child_crop.top + (top_diff * vertical_scale_factor),
                            left: child_crop.left + (left_diff * horizontal_scale_factor),
                            width: cropped_width * horizontal_scale_factor,
                            height: cropped_height * vertical_scale_factor,
                        };

                        RenderLayout {
                            top: self.top + (cropped_top * self.scale_y),
                            left: self.left + (cropped_left * self.scale_x),
                            width: cropped_width * self.scale_x,
                            height: cropped_height * self.scale_y,
                            rotation_degrees: child.rotation_degrees + self.rotation_degrees, // TODO: not exactly correct
                            content: RenderLayoutContent::ChildNode {
                                index,
                                crop,
                                border_color,
                                border_width,
                            },
                            border_radius: child.border_radius * unified_scale,
                            masks: self.parent_parent_masks(&child.masks),
                        }
                    }
                    RenderLayoutContent::BoxShadow { color, blur_radius } => {
                        RenderLayout {
                            top: self.top + (cropped_top * self.scale_y),
                            left: self.left + (cropped_left * self.scale_x),
                            width: cropped_width * self.scale_x,
                            height: cropped_height * self.scale_y,
                            rotation_degrees: child.rotation_degrees + self.rotation_degrees, // TODO: not exactly correct
                            content: RenderLayoutContent::BoxShadow {
                                color,
                                blur_radius: blur_radius * unified_scale,
                            },
                            border_radius: child.border_radius * unified_scale,
                            masks: self.parent_parent_masks(&child.masks),
                        }
                    }
                }
            }
        }
    }

    /// Calculate RenderLayout for self (without children)
    /// Resulting layout is in coordinates:
    /// - relative self's parent top-left corner.
    /// - before parent scaling is applied
    fn render_layout(&self, parent_masks: &[Mask]) -> RenderLayout {
        RenderLayout {
            top: self.top,
            left: self.left,
            width: self.width,
            height: self.height,
            rotation_degrees: self.rotation_degrees,
            content: match self.content {
                LayoutContent::Color(color) => RenderLayoutContent::Color {
                    color,
                    border_color: self.border_color,
                    border_width: self.border_width,
                },
                LayoutContent::ChildNode { index, size } => RenderLayoutContent::ChildNode {
                    index,
                    crop: Crop {
                        top: 0.0,
                        left: 0.0,
                        width: size.width,
                        height: size.height,
                    },
                    border_color: self.border_color,
                    border_width: self.border_width,
                },
                LayoutContent::None => RenderLayoutContent::Color {
                    color: RGBAColor(0, 0, 0, 0),
                    border_color: self.border_color,
                    border_width: self.border_width,
                },
            },
            border_radius: self.border_radius,
            masks: parent_masks.to_vec(),
        }
    }

    /// calculate RenderLayout for one of self box shadows
    fn box_shadow_layout(&self, box_shadow: &BoxShadow, parent_masks: &[Mask]) -> RenderLayout {
        RenderLayout {
            top: self.top + box_shadow.offset_y,
            left: self.left + box_shadow.offset_x,
            width: self.width,
            height: self.height,
            rotation_degrees: self.rotation_degrees, // TODO: this is incorrect
            border_radius: self.border_radius + (box_shadow.blur_radius / 2.0),
            content: RenderLayoutContent::BoxShadow {
                color: box_shadow.color,
                blur_radius: box_shadow.blur_radius,
            },
            masks: parent_masks.to_vec(),
        }
    }

    /// Calculate ParentMasks in coordinates of child NestedLayout.
    fn child_parent_masks(&self, masks: &[Mask]) -> Vec<Mask> {
        masks
            .iter()
            .map(|mask| Mask {
                radius: mask.radius / f32::min(self.scale_x, self.scale_y),
                top: (mask.top - self.top) / self.scale_y,
                left: (mask.left - self.left) / self.scale_x,
                width: mask.width / self.scale_x,
                height: mask.height / self.scale_y,
            })
            .collect()
    }

    /// Translates parent mask from child coordinates to parent. Reverse operation to `child_parent_masks`.
    fn parent_parent_masks(&self, masks: &[Mask]) -> Vec<Mask> {
        masks
            .iter()
            .map(|mask| Mask {
                radius: mask.radius * f32::min(self.scale_x, self.scale_y),
                top: (mask.top * self.scale_y) + self.top,
                left: (mask.left * self.scale_x) + self.left,
                width: mask.width * self.scale_x,
                height: mask.height * self.scale_y,
            })
            .collect()
    }
}
