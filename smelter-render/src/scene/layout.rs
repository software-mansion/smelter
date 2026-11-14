use std::time::Duration;

use crate::{
    Resolution,
    state::{InputTextures, RenderCtx, node_texture::NodeTexture},
    transformations::layout::{LayoutContent, LayoutNode, NestedLayout},
};

use super::{
    AbsolutePosition, BorderRadius, ComponentId, HorizontalPosition, Position, RGBAColor,
    SceneError, Size, StatefulComponent, VerticalPosition,
    rescaler_component::StatefulRescalerComponent,
    scene_state::{BuildStateTreeCtx, LayoutCtx},
    tiles_component::StatefulTilesComponent,
    view_component::StatefulViewComponent,
};

pub(super) enum StatefulLayoutComponent {
    View(StatefulViewComponent),
    Tiles(StatefulTilesComponent),
    Rescaler(StatefulRescalerComponent),
}

/// Child of a layout component.
pub(super) enum LayoutChild {
    /// Layout component that is rendered to the same texture as its parent.
    Layout(Box<StatefulLayoutComponent>),
    /// Component that is rendered to a separate texture. The parent only decides where
    /// that texture is placed.
    Node(Box<StatefulComponent>),
}

/// Layout component that is rendered to its own texture. Layout components nested directly
/// inside of it are rendered to the same texture.
pub(super) struct LayoutRoot {
    pub(super) component: StatefulLayoutComponent,
    /// Size that is available for the component.
    size: Size,
    /// Result of the last `layout` call.
    layout: Option<NestedLayout>,
    /// Resolution of the output texture calculated by the last `layout` call.
    resolution: Resolution,
    node: LayoutNode,
    pub(super) output: NodeTexture,
}

impl LayoutRoot {
    pub(super) fn new(
        component: StatefulLayoutComponent,
        forced_size: Option<Size>,
        ctx: &BuildStateTreeCtx,
    ) -> Result<Self, SceneError> {
        let pts = ctx.last_render_pts;
        let size = match forced_size {
            Some(size) => size,
            None => {
                let (width, height) = match component.position(pts) {
                    Position::Static { width, height } => (width, height),
                    // Technically absolute positioning is a bug here, but I think throwing error
                    // in this case would be to invasive. It's better to just ignore those values.
                    Position::Absolute(position) => (position.width, position.height),
                };
                let (Some(width), Some(height)) = (width, height) else {
                    return Err(SceneError::UnknownDimensionsForLayoutNodeRoot {
                        component: component.component_type(),
                        msg: match component.component_id() {
                            Some(id) => format!(
                                "Please provide width and height values for component with id \"{id}\""
                            ),
                            None => "Please provide width and height values.".to_string(),
                        },
                    });
                };
                Size { width, height }
            }
        };

        Ok(Self {
            resolution: resolution(&component, size, pts),
            component,
            size,
            layout: None,
            node: LayoutNode::new(ctx.render_ctx),
            output: NodeTexture::new(),
        })
    }

    pub(super) fn size(&self) -> Size {
        self.resolution.into()
    }

    pub(super) fn layout(&mut self, ctx: &LayoutCtx) {
        self.component
            .for_each_node_mut(&mut |node| node.layout(ctx));
        self.resolution = resolution(&self.component, self.size, ctx.pts);
        self.layout = Some(self.component.layout(self.size, ctx.pts));
    }

    pub(super) fn render(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        self.component
            .for_each_node_mut(&mut |node| node.render(ctx, inputs, pts));
        let Some(layout) = self.layout.take() else {
            return;
        };

        let mut sources = Vec::new();
        self.component
            .for_each_node(&mut |node| sources.push(node.texture(inputs)));
        self.node
            .render(ctx, &sources, &mut self.output, layout, self.resolution);
    }
}

/// Resolution of a texture that `component` is rendered to if `size` is available for it.
fn resolution(component: &StatefulLayoutComponent, size: Size, pts: Duration) -> Resolution {
    match component.position(pts) {
        Position::Static { width, height } => Size {
            width: width.unwrap_or(size.width),
            height: height.unwrap_or(size.height),
        },
        Position::Absolute(position) => Size {
            width: position.width.unwrap_or(size.width),
            height: position.height.unwrap_or(size.height),
        },
    }
    .into()
}

impl LayoutChild {
    pub(super) fn width(&self, pts: Duration) -> Option<f32> {
        match self {
            LayoutChild::Layout(layout) => match layout.position(pts) {
                Position::Static { width, .. } => width,
                Position::Absolute(position) => position.width,
            },
            LayoutChild::Node(node) => Some(node.size().width),
        }
    }

    pub(super) fn height(&self, pts: Duration) -> Option<f32> {
        match self {
            LayoutChild::Layout(layout) => match layout.position(pts) {
                Position::Static { height, .. } => height,
                Position::Absolute(position) => position.height,
            },
            LayoutChild::Node(node) => Some(node.size().height),
        }
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        match self {
            LayoutChild::Layout(layout) => layout.component_id(),
            LayoutChild::Node(node) => node.component_id(),
        }
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

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        match self {
            StatefulLayoutComponent::View(view) => view.component_id(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.component_id(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.component_id(),
        }
    }

    pub(super) fn component_type(&self) -> &'static str {
        match self {
            StatefulLayoutComponent::View(_) => "View",
            StatefulLayoutComponent::Tiles(_) => "Tiles",
            StatefulLayoutComponent::Rescaler(_) => "Rescaler",
        }
    }

    pub(super) fn children(&self) -> Vec<&LayoutChild> {
        match self {
            StatefulLayoutComponent::View(view) => view.children(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children(),
        }
    }

    pub(super) fn children_mut(&mut self) -> Vec<&mut LayoutChild> {
        match self {
            StatefulLayoutComponent::View(view) => view.children_mut(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children_mut(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children_mut(),
        }
    }

    /// Calls `f` for every component that is rendered to a separate texture and placed by
    /// this component or by layout components nested inside of it.
    pub(super) fn for_each_node<'a>(&'a self, f: &mut impl FnMut(&'a StatefulComponent)) {
        for child in self.children() {
            match child {
                LayoutChild::Layout(layout) => layout.for_each_node(f),
                LayoutChild::Node(node) => f(node),
            }
        }
    }

    /// Mutable version of `for_each_node`.
    pub(super) fn for_each_node_mut(&mut self, f: &mut impl FnMut(&mut StatefulComponent)) {
        for child in self.children_mut() {
            match child {
                LayoutChild::Layout(layout) => layout.for_each_node_mut(f),
                LayoutChild::Node(node) => f(node),
            }
        }
    }

    pub(super) fn nodes_count(&self) -> usize {
        let mut count = 0;
        self.for_each_node(&mut |_| count += 1);
        count
    }

    pub(super) fn layout_content(component: &LayoutChild, index: usize) -> LayoutContent {
        match component {
            LayoutChild::Layout(_layout) => LayoutContent::None,
            LayoutChild::Node(node) => LayoutContent::ChildNode {
                index,
                size: node.size(),
            },
        }
    }

    pub(super) fn layout_absolute_position_child(
        child: &mut LayoutChild,
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
            LayoutChild::Layout(layout_component) => {
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
            LayoutChild::Node(_) => {
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
