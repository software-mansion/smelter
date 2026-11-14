use crate::{
    scene::{
        Overflow, Position, Size, ViewChildrenDirection,
        layout::{ChildPlacement, LayoutChild},
    },
    transformations::layout::{LayoutContent, Mask, NestedLayout},
};

use super::ViewComponentParam;

#[derive(Debug)]
struct StaticChildLayoutOpts {
    width: Option<f32>,
    height: Option<f32>,
    /// Offset inside parent component (position where next static child should start).
    static_offset: f32,
    /// Define size(width or height) of a static component if it's not
    /// already defined explicitly.
    /// For direction=row defines width of a static component
    /// For direction=column defines height of a static component
    static_child_size: f32,
    parent_size: Size,
    /// border width before rescaling, it is used to calculate top/left offset correctly
    /// when `overflow: fit` is set
    parent_border_width: f32,
}

impl ViewComponentParam {
    /// Places `children` inside of the view and returns a layout of the view itself.
    pub(super) fn place(
        &self,
        size: Size, // how much size component has available(includes space for border)
        children: &mut [LayoutChild],
    ) -> NestedLayout {
        let content_size = Size {
            width: f32::max(size.width - 2.0 * self.border_width, 0.0),
            height: f32::max(size.height - 2.0 * self.border_width, 0.0),
        };
        let border_radius = self.border_radius.clip_to_size(size);
        let static_child_size = self.static_child_size(content_size, children);
        let (scale, crop, mask) = match self.overflow {
            Overflow::Visible => (1.0, None, None),
            Overflow::Hidden => (
                1.0,
                None,
                Some(Mask {
                    radius: border_radius - self.border_width,
                    top: self.border_width,
                    left: self.border_width,
                    width: content_size.width,
                    height: content_size.height,
                }),
            ),
            Overflow::Fit => (
                self.scale_factor_for_overflow_fit(content_size, children),
                None,
                Some(Mask {
                    radius: border_radius - self.border_width,
                    top: self.border_width,
                    left: self.border_width,
                    width: content_size.width,
                    height: content_size.height,
                }),
            ),
        };

        // offset along x or y direction (depends on self.direction) where next
        // child component should be placed
        let mut static_offset = self.border_width / scale;

        for child in children.iter_mut() {
            let placement = match child.position() {
                Position::Static { width, height } => {
                    let (placement, updated_static_offset) =
                        self.static_child_placement(StaticChildLayoutOpts {
                            width,
                            height,
                            static_offset,
                            static_child_size,
                            parent_size: content_size,
                            parent_border_width: self.border_width / scale,
                        });

                    static_offset = updated_static_offset;
                    placement
                }
                Position::Absolute(position) => ChildPlacement::absolute(position, size),
            };
            child.place(placement);
        }

        NestedLayout {
            top: 0.0,
            left: 0.0,
            width: size.width,
            height: size.height,
            rotation_degrees: 0.0,
            scale_x: scale,
            scale_y: scale,
            crop,
            mask,
            content: LayoutContent::Color(self.background_color),
            children: vec![],
            border_width: self.border_width,
            border_color: self.border_color,
            border_radius,
            box_shadow: self.box_shadow.clone(),
        }
    }

    fn static_child_placement(&self, opts: StaticChildLayoutOpts) -> (ChildPlacement, f32) {
        let mut static_offset = opts.static_offset;
        let (top, left, width, height) = match self.direction {
            ViewChildrenDirection::Row => {
                let width = opts.width.unwrap_or(opts.static_child_size);
                let height = opts
                    .height
                    .unwrap_or(opts.parent_size.height - self.padding.vertical());
                let top = opts.parent_border_width + self.padding.top;
                let left = static_offset + self.padding.left;
                static_offset += width;
                (top, left, width, height)
            }
            ViewChildrenDirection::Column => {
                let height = opts.height.unwrap_or(opts.static_child_size);
                let width = opts
                    .width
                    .unwrap_or(opts.parent_size.width - self.padding.horizontal());
                let top = static_offset + self.padding.top;
                let left = opts.parent_border_width + self.padding.left;
                static_offset += height;
                (top, left, width, height)
            }
        };

        let placement = ChildPlacement::new(top, left, Size { width, height });
        (placement, static_offset)
    }

    /// Calculate a size of a static child component that does not have it explicitly defined.
    /// Returned value represents width if the direction is `ViewChildrenDirection::Row` or
    /// height if the direction is `ViewChildrenDirection::Column`.
    ///
    /// size represents dimensions of content (without a border).
    fn static_child_size(&self, size: Size, children: &[LayoutChild]) -> f32 {
        let max_size = match self.direction {
            super::ViewChildrenDirection::Row => size.width - self.padding.horizontal(),
            super::ViewChildrenDirection::Column => size.height - self.padding.vertical(),
        };

        let children_with_unknown_size_count = Self::static_children_iter(children)
            .filter(|child| match self.direction {
                ViewChildrenDirection::Row => child.width().is_none(),
                ViewChildrenDirection::Column => child.height().is_none(),
            })
            .count();
        let static_children_sum = self.sum_static_children_sizes(children);

        if children_with_unknown_size_count == 0 {
            return 0.0; // if there is no dynamically sized children then this value does not matter
        }
        f32::max(
            0.0,
            (max_size - static_children_sum) / children_with_unknown_size_count as f32,
        )
    }

    fn scale_factor_for_overflow_fit(&self, content_size: Size, children: &[LayoutChild]) -> f32 {
        let sum_size = self.sum_static_children_sizes(children).max(0.000000001); // avoid division by 0
        let (max_size, max_alternative_size) = match self.direction {
            super::ViewChildrenDirection::Row => (content_size.width, content_size.height),
            super::ViewChildrenDirection::Column => (content_size.height, content_size.width),
        };
        let max_alternative_size_for_child = Self::static_children_iter(children)
            .map(|child| match self.direction {
                ViewChildrenDirection::Row => child.height().unwrap_or(0.0),
                ViewChildrenDirection::Column => child.width().unwrap_or(0.0),
            })
            .max_by(|a, b| f32::partial_cmp(a, b).unwrap()) // will panic if comparing NaN
            .unwrap_or(0.0)
            .max(0.000000001); // avoid division by 0

        f32::min(
            1.0,
            f32::min(
                max_size / sum_size,
                max_alternative_size / max_alternative_size_for_child,
            ),
        )
    }

    fn sum_static_children_sizes(&self, children: &[LayoutChild]) -> f32 {
        let size_accessor = match self.direction {
            super::ViewChildrenDirection::Row => LayoutChild::width,
            super::ViewChildrenDirection::Column => LayoutChild::height,
        };

        Self::static_children_iter(children)
            .map(|component| size_accessor(component).unwrap_or(0.0))
            .sum()
    }

    fn static_children_iter(children: &[LayoutChild]) -> impl Iterator<Item = &LayoutChild> {
        children.iter().filter(|child| match child.position() {
            Position::Static { .. } => true,
            Position::Absolute(_) => false,
        })
    }
}
