use std::time::Duration;

use crate::{
    transformations::layout::{self, LayoutContent, NestedLayout},
    Resolution,
};

use super::{
    rescaler_component::StatefulRescalerComponent, tiles_component::StatefulTilesComponent,
    view_component::StatefulViewComponent, AbsolutePosition, BorderRadius, ComponentId,
    HorizontalPosition, Position, RGBAColor, Size, StatefulComponent, VerticalPosition,
};

#[derive(Debug, Clone)]
pub(super) enum StatefulLayoutComponent {
    View(StatefulViewComponent),
    Tiles(StatefulTilesComponent),
    Rescaler(StatefulRescalerComponent),
}

#[derive(Debug)]
pub(crate) struct SizedLayoutComponent {
    component: Box<StatefulLayoutComponent>,
    size: Size,
}

#[derive(Debug)]
pub(crate) struct LayoutNode {
    pub(crate) root: SizedLayoutComponent,
}

impl layout::LayoutProvider for LayoutNode {
    fn layouts(&mut self, pts: std::time::Duration, inputs: &[Option<Resolution>]) -> NestedLayout {
        self.root.component.update_state(inputs);
        self.root.layout(pts)
    }

    fn resolution(&self, pts: Duration) -> Resolution {
        self.root.resolution(pts)
    }
}

impl StatefulLayoutComponent {
    pub(super) fn layout(&mut self, size: Size, pts: Duration) -> NestedLayout {
        match self {
            StatefulLayoutComponent::View(view) => view.layout(size, pts),
            StatefulLayoutComponent::Tiles(tiles) => tiles.layout(size, pts),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.layout(size, pts),
        }
    }

    // External position of a component (includes border, padding, ...)
    pub(super) fn position(&self, pts: Duration) -> Position {
        match self {
            StatefulLayoutComponent::View(view) => view.position(pts),
            StatefulLayoutComponent::Tiles(tiles) => tiles.position(pts),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.position(pts),
        }
    }

    pub(crate) fn component_id(&self) -> Option<&ComponentId> {
        match self {
            StatefulLayoutComponent::View(view) => view.component_id(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.component_id(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.component_id(),
        }
    }

    pub(crate) fn component_type(&self) -> &'static str {
        match self {
            StatefulLayoutComponent::View(_) => "View",
            StatefulLayoutComponent::Tiles(_) => "Tiles",
            StatefulLayoutComponent::Rescaler(_) => "Rescaler",
        }
    }

    pub(super) fn children(&self) -> Vec<&StatefulComponent> {
        match self {
            StatefulLayoutComponent::View(view) => view.children(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children(),
        }
    }

    pub(super) fn children_mut(&mut self) -> Vec<&mut StatefulComponent> {
        match self {
            StatefulLayoutComponent::View(view) => view.children_mut(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children_mut(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children_mut(),
        }
    }

    pub(super) fn node_children(&self) -> Vec<&StatefulComponent> {
        self.children()
            .into_iter()
            .flat_map(|child| match child {
                StatefulComponent::Layout(layout) => layout.node_children(),
                _ => vec![child],
            })
            .collect()
    }

    pub(super) fn update_state(&mut self, input_resolutions: &[Option<Resolution>]) {
        let mut child_index_offset = 0;
        for child in self.children_mut().iter_mut() {
            match child {
                StatefulComponent::InputStream(input) => {
                    // TODO
                    input.size = input_resolutions[child_index_offset]
                        .map(Into::into)
                        .unwrap_or(Size {
                            width: 0.0,
                            height: 0.0,
                        });
                    child_index_offset += 1;
                }
                StatefulComponent::Shader(_)
                | StatefulComponent::Image(_)
                | StatefulComponent::Text(_)
                | StatefulComponent::WebView(_) => {
                    child_index_offset += 1; // no state
                }
                StatefulComponent::Layout(layout) => {
                    let node_children = layout.node_children().len();
                    layout.update_state(
                        &input_resolutions[child_index_offset..child_index_offset + node_children],
                    );
                    child_index_offset += node_children
                }
            }
        }
    }

    pub(super) fn layout_content(component: &StatefulComponent, index: usize) -> LayoutContent {
        match component {
            StatefulComponent::Layout(_layout) => LayoutContent::None,
            StatefulComponent::InputStream(input) => LayoutContent::ChildNode {
                index,
                size: input.size,
            },
            StatefulComponent::Shader(shader) => LayoutContent::ChildNode {
                index,
                size: shader.component.size,
            },
            StatefulComponent::WebView(web) => LayoutContent::ChildNode {
                index,
                size: web.size(),
            },
            StatefulComponent::Image(image) => LayoutContent::ChildNode {
                index,
                size: image.size(),
            },
            StatefulComponent::Text(text) => LayoutContent::ChildNode {
                index,
                size: text.size(),
            },
        }
    }

    pub(super) fn layout_absolute_position_child(
        child: &mut StatefulComponent,
        position: AbsolutePosition,
        parent_size: Size,
        pts: Duration,
    ) -> NestedLayout {
        let width = position.width.unwrap_or(parent_size.width);
        let height = position.height.unwrap_or(parent_size.height);

        let top = match position.position_vertical {
            VerticalPosition::TopOffset(top) => top,
            VerticalPosition::BottomOffset(bottom) => parent_size.height - bottom - height,
        };
        let left = match position.position_horizontal {
            HorizontalPosition::LeftOffset(left) => left,
            HorizontalPosition::RightOffset(right) => parent_size.width - right - width,
        };

        let rotation_degrees = position.rotation_degrees;
        let content = Self::layout_content(child, 0);
        let crop = None;
        let mask = None;

        match child {
            StatefulComponent::Layout(layout_component) => {
                let children_layouts = layout_component.layout(Size { width, height }, pts);
                let child_nodes_count = match content {
                    LayoutContent::ChildNode { .. } => children_layouts.child_nodes_count + 1,
                    _ => children_layouts.child_nodes_count,
                };
                NestedLayout {
                    top,
                    left,
                    width,
                    height,
                    rotation_degrees,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    crop,
                    mask,

                    content,
                    child_nodes_count,
                    children: vec![children_layouts],
                    border_width: 0.0,
                    border_color: RGBAColor(0, 0, 0, 0),
                    border_radius: BorderRadius::ZERO,
                    box_shadow: vec![],
                }
            }
            _non_layout_components => {
                let child_nodes_count = match content {
                    LayoutContent::ChildNode { .. } => 1,
                    _ => 0,
                };

                NestedLayout {
                    top,
                    left,
                    width,
                    height,
                    rotation_degrees,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    crop,
                    mask,

                    content,
                    child_nodes_count,
                    children: vec![],
                    border_width: 0.0,
                    border_color: RGBAColor(0, 0, 0, 0),
                    border_radius: BorderRadius::ZERO,
                    box_shadow: vec![],
                }
            }
        }
    }
}

impl SizedLayoutComponent {
    pub(super) fn new(component: Box<StatefulLayoutComponent>, size: Size) -> Self {
        Self { component, size }
    }

    fn resolution(&self, pts: Duration) -> Resolution {
        match self.component.position(pts) {
            Position::Static { width, height } => Size {
                width: width.unwrap_or(self.size.width),
                height: height.unwrap_or(self.size.height),
            },
            Position::Absolute(position) => Size {
                width: position.width.unwrap_or(self.size.width),
                height: position.height.unwrap_or(self.size.height),
            },
        }
        .into()
    }

    fn layout(&mut self, pts: Duration) -> NestedLayout {
        self.component.layout(self.size, pts)
    }
}
