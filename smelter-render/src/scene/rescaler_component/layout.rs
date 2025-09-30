use std::time::Duration;

use crate::{
    scene::{
        layout::StatefulLayoutComponent, BorderRadius, HorizontalAlign, RGBAColor, RescaleMode,
        Size, StatefulComponent, VerticalAlign,
    },
    transformations::layout::{LayoutContent, Mask, NestedLayout},
};

use super::RescalerComponentParam;

impl RescalerComponentParam {
    pub(super) fn layout(
        &self,
        size: Size,
        child: &mut StatefulComponent,
        pts: Duration,
    ) -> NestedLayout {
        let content_size = Size {
            width: f32::max(size.width - (2.0 * self.border_width), 0.0),
            height: f32::max(size.height - (2.0 * self.border_width), 0.0),
        };
        let child_width = child.width(pts);
        let child_height = child.height(pts);
        let border_radius = self.border_radius.clip_to_size(size);
        match (child_width, child_height) {
            (None, None) => self.layout_with_scale(content_size, border_radius, child, pts, 1.0),
            (None, Some(child_height)) => self.layout_with_scale(
                content_size,
                border_radius,
                child,
                pts,
                content_size.height / child_height,
            ),
            (Some(child_width), None) => self.layout_with_scale(
                content_size,
                border_radius,
                child,
                pts,
                content_size.width / child_width,
            ),
            (Some(child_width), Some(child_height)) => {
                let scale = match self.mode {
                    RescaleMode::Fit => f32::min(
                        content_size.width / child_width,
                        content_size.height / child_height,
                    ),
                    RescaleMode::Fill => f32::max(
                        content_size.width / child_width,
                        content_size.height / child_height,
                    ),
                };
                self.layout_with_scale(content_size, border_radius, child, pts, scale)
            }
        }
    }

    fn layout_with_scale(
        &self,
        max_size: Size, // without borders
        border_radius: BorderRadius,
        child: &mut StatefulComponent,
        pts: Duration,
        scale: f32,
    ) -> NestedLayout {
        let child_width = child.width(pts);
        let child_height = child.height(pts);
        let (content, children, child_nodes_count) = match child {
            StatefulComponent::Layout(layout_component) => {
                let children_layout = layout_component.layout(
                    Size {
                        width: child_width.unwrap_or(max_size.width / scale),
                        height: child_height.unwrap_or(max_size.height / scale),
                    },
                    pts,
                );
                let child_nodes_count = children_layout.child_nodes_count;
                (
                    LayoutContent::None,
                    vec![children_layout],
                    child_nodes_count,
                )
            }
            ref _non_layout => (StatefulLayoutComponent::layout_content(child, 0), vec![], 1),
        };

        let top = match self.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Bottom => child
                .height(pts)
                .map(|height| max_size.height - (height * scale))
                .unwrap_or(0.0),
            VerticalAlign::Center | VerticalAlign::Justified => child
                .height(pts)
                .map(|height| (max_size.height - (height * scale)) / 2.0)
                .unwrap_or(0.0),
        };
        let left = match self.horizontal_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Right => child
                .width(pts)
                .map(|width| max_size.width - (width * scale))
                .unwrap_or(0.0),
            HorizontalAlign::Center | HorizontalAlign::Justified => child
                .width(pts)
                .map(|width| (max_size.width - (width * scale)) / (2.0))
                .unwrap_or(0.0),
        };

        let width = child
            .width(pts)
            .map(|child_width| child_width * scale)
            .unwrap_or(max_size.width);
        let height = child
            .height(pts)
            .map(|child_height| child_height * scale)
            .unwrap_or(max_size.height);

        NestedLayout {
            top: 0.0,
            left: 0.0,
            width: max_size.width + (self.border_width * 2.0),
            height: max_size.height + (self.border_width * 2.0),
            rotation_degrees: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            crop: None,
            mask: Some(Mask {
                radius: border_radius - self.border_width,
                top: self.border_width,
                left: self.border_width,
                width: max_size.width,
                height: max_size.height,
            }),
            content: LayoutContent::None,
            children: vec![NestedLayout {
                top: top + self.border_width,
                left: left + self.border_width,
                width,
                height,
                rotation_degrees: 0.0,
                scale_x: scale,
                scale_y: scale,
                crop: None,
                mask: None,
                content,
                child_nodes_count,
                children,
                border_width: 0.0,
                border_color: RGBAColor(0, 0, 0, 0),
                border_radius: BorderRadius::ZERO,
                box_shadow: vec![],
            }],
            child_nodes_count,
            border_width: self.border_width,
            border_color: self.border_color,
            border_radius,
            box_shadow: self.box_shadow.clone(),
        }
    }
}
