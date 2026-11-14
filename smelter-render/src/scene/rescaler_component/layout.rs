use crate::{
    scene::{
        HorizontalAlign, RescaleMode, Size, VerticalAlign,
        layout::{ChildPlacement, LayoutChild},
    },
    transformations::layout::{LayoutContent, Mask, NestedLayout},
};

use super::RescalerComponentParam;

impl RescalerComponentParam {
    /// Places `child` inside of the rescaler and returns a layout of the rescaler itself.
    pub(super) fn place(&self, size: Size, child: &mut LayoutChild) -> NestedLayout {
        // without borders
        let max_size = Size {
            width: f32::max(size.width - (2.0 * self.border_width), 0.0),
            height: f32::max(size.height - (2.0 * self.border_width), 0.0),
        };
        let child_width = child.width();
        let child_height = child.height();
        let border_radius = self.border_radius.clip_to_size(size);
        let scale = match (child_width, child_height) {
            (None, None) => 1.0,
            (None, Some(child_height)) => max_size.height / child_height,
            (Some(child_width), None) => max_size.width / child_width,
            (Some(child_width), Some(child_height)) => match self.mode {
                RescaleMode::Fit => {
                    f32::min(max_size.width / child_width, max_size.height / child_height)
                }
                RescaleMode::Fill => {
                    f32::max(max_size.width / child_width, max_size.height / child_height)
                }
            },
        };

        let top = match self.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Bottom => child_height
                .map(|height| max_size.height - (height * scale))
                .unwrap_or(0.0),
            VerticalAlign::Center | VerticalAlign::Justified => child_height
                .map(|height| (max_size.height - (height * scale)) / 2.0)
                .unwrap_or(0.0),
        };
        let left = match self.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Right => child_width
                .map(|width| max_size.width - (width * scale))
                .unwrap_or(0.0),
            HorizontalAlign::Center | HorizontalAlign::Justified => child_width
                .map(|width| (max_size.width - (width * scale)) / (2.0))
                .unwrap_or(0.0),
        };

        child.place(ChildPlacement {
            top: top + self.border_width,
            left: left + self.border_width,
            width: child_width
                .map(|child_width| child_width * scale)
                .unwrap_or(max_size.width),
            height: child_height
                .map(|child_height| child_height * scale)
                .unwrap_or(max_size.height),
            rotation_degrees: 0.0,
            content_size: Size {
                width: child_width.unwrap_or(max_size.width / scale),
                height: child_height.unwrap_or(max_size.height / scale),
            },
            scale,
        });

        NestedLayout {
            top: 0.0,
            left: 0.0,
            width: max_size.width + (self.border_width * 2.0),
            height: max_size.height + (self.border_width * 2.0),
            rotation_degrees: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            mask: Some(Mask {
                radius: border_radius - self.border_width,
                top: self.border_width,
                left: self.border_width,
                width: max_size.width,
                height: max_size.height,
            }),
            content: LayoutContent::None,
            children: vec![],
            border_width: self.border_width,
            border_color: self.border_color,
            border_radius,
            box_shadow: self.box_shadow.clone(),
        }
    }
}
