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

/// Layout component with its state.
///
/// Layout of a frame is calculated in two steps:
/// - `measure` - Children are measured before their parent. It resolves everything that
///   depends on time (transitions) or on inputs (resolution of a frame), so after this step
///   a component knows what size it asks for (`position`), and it can rely on sizes of its
///   children to do that.
/// - `place` - Parent is placed before its children. A component gets its final size and
///   decides where its children are placed.
pub(super) enum StatefulLayoutComponent {
    View(StatefulViewComponent),
    Tiles(StatefulTilesComponent),
    Rescaler(StatefulRescalerComponent),
}

/// Child of a layout component.
pub(super) struct LayoutChild {
    pub(super) component: ChildComponent,
    /// Where the parent placed this child in the current frame. `None` if the child is
    /// not shown.
    placement: Option<ChildPlacement>,
}

pub(super) enum ChildComponent {
    /// Layout component that is rendered to the same texture as its parent.
    Layout(Box<StatefulLayoutComponent>),
    /// Component that is rendered to a separate texture. The parent only decides where
    /// that texture is placed.
    Node(Box<StatefulComponent>),
}

/// Area inside of a parent component that is assigned to a child.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChildPlacement {
    /// Position of the top-left corner in coordinates of the parent.
    pub(super) top: f32,
    pub(super) left: f32,
    /// Size of the area in coordinates of the parent.
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) rotation_degrees: f32,
    /// Size that the child has available for its own layout. The result of that layout is
    /// scaled by `scale` to fill the area.
    pub(super) content_size: Size,
    pub(super) scale: f32,
}

/// Layout component that is rendered to its own texture. Layout components nested directly
/// inside of it are rendered to the same texture.
pub(super) struct LayoutRoot {
    pub(super) component: StatefulLayoutComponent,
    /// Size that is available for the component.
    size: Size,
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
        let size = match forced_size {
            Some(size) => size,
            None => {
                let (width, height) = match component.position() {
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
            resolution: resolution(&component, size),
            component,
            size,
            node: LayoutNode::new(ctx.render_ctx),
            output: NodeTexture::new(),
        })
    }

    pub(super) fn size(&self) -> Size {
        self.resolution.into()
    }

    pub(super) fn layout(&mut self, ctx: &LayoutCtx) {
        self.component.measure(ctx);
        self.resolution = resolution(&self.component, self.size);
        self.component.place(self.size);
    }

    pub(super) fn render(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        self.component.render_nodes(ctx, inputs, pts);

        let mut sources = Vec::new();
        let Some(layout) = self.component.nested_layout(inputs, &mut sources) else {
            return;
        };
        self.node
            .render(ctx, &sources, &mut self.output, layout, self.resolution);
    }
}

/// Resolution of a texture that `component` is rendered to if `size` is available for it.
fn resolution(component: &StatefulLayoutComponent, size: Size) -> Resolution {
    match component.position() {
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
    pub(super) fn new(component: ChildComponent) -> Self {
        Self {
            component,
            placement: None,
        }
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        match &self.component {
            ChildComponent::Layout(layout) => layout.component_id(),
            ChildComponent::Node(node) => node.component_id(),
        }
    }

    pub(super) fn measure(&mut self, ctx: &LayoutCtx) {
        match &mut self.component {
            ChildComponent::Layout(layout) => layout.measure(ctx),
            // The parent does not affect anything inside of a node, so its entire layout
            // can be calculated here.
            ChildComponent::Node(node) => node.layout(ctx),
        }
    }

    /// Position and size that the child asks for. `measure` needs to be called first.
    pub(super) fn position(&self) -> Position {
        match &self.component {
            ChildComponent::Layout(layout) => layout.position(),
            ChildComponent::Node(node) => {
                let size = node.size();
                Position::Static {
                    width: Some(size.width),
                    height: Some(size.height),
                }
            }
        }
    }

    /// Width that the child asks for. `None` if the parent can choose any value.
    pub(super) fn width(&self) -> Option<f32> {
        match self.position() {
            Position::Static { width, .. } => width,
            Position::Absolute(position) => position.width,
        }
    }

    /// Height that the child asks for. `None` if the parent can choose any value.
    pub(super) fn height(&self) -> Option<f32> {
        match self.position() {
            Position::Static { height, .. } => height,
            Position::Absolute(position) => position.height,
        }
    }

    /// Shows the child in the `placement` area of its parent.
    pub(super) fn place(&mut self, placement: ChildPlacement) {
        if let ChildComponent::Layout(layout) = &mut self.component {
            layout.place(placement.content_size);
        }
        self.placement = Some(placement);
    }

    /// Does not show the child in the current frame.
    pub(super) fn hide(&mut self) {
        self.placement = None;
    }

    /// Layout of the child in a form that can be rendered. `None` if the child is not shown.
    ///
    /// Textures that are used by the returned layout are added to `sources`.
    fn nested_layout<'a>(
        &'a self,
        inputs: &'a InputTextures,
        sources: &mut Vec<&'a NodeTexture>,
    ) -> Option<NestedLayout> {
        let placement = self.placement?;
        let (content, children) = match &self.component {
            ChildComponent::Layout(layout) => (
                LayoutContent::None,
                vec![layout.nested_layout(inputs, sources)?],
            ),
            ChildComponent::Node(node) => {
                let index = sources.len();
                sources.push(node.texture(inputs));
                let size = node.size();
                (LayoutContent::ChildNode { index, size }, vec![])
            }
        };

        Some(NestedLayout {
            top: placement.top,
            left: placement.left,
            width: placement.width,
            height: placement.height,
            rotation_degrees: placement.rotation_degrees,
            scale_x: placement.scale,
            scale_y: placement.scale,
            crop: None,
            mask: None,
            content,
            children,
            border_width: 0.0,
            border_color: RGBAColor(0, 0, 0, 0),
            border_radius: BorderRadius::ZERO,
            box_shadow: vec![],
        })
    }
}

impl ChildPlacement {
    /// Area for a child that is not scaled or rotated.
    pub(super) fn new(top: f32, left: f32, size: Size) -> Self {
        Self {
            top,
            left,
            width: size.width,
            height: size.height,
            rotation_degrees: 0.0,
            content_size: size,
            scale: 1.0,
        }
    }

    /// Area for a child that is positioned absolutely inside of a parent with `parent_size`.
    pub(super) fn absolute(position: AbsolutePosition, parent_size: Size) -> Self {
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

        Self {
            rotation_degrees: position.rotation_degrees,
            ..Self::new(top, left, Size { width, height })
        }
    }
}

impl StatefulLayoutComponent {
    /// First step of the layout. Measures the component and everything inside of it for
    /// a frame described by `ctx`.
    pub(super) fn measure(&mut self, ctx: &LayoutCtx) {
        match self {
            StatefulLayoutComponent::View(view) => view.measure(ctx),
            StatefulLayoutComponent::Tiles(tiles) => tiles.measure(ctx),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.measure(ctx),
        }
    }

    /// Second step of the layout. Places the component and everything inside of it in
    /// an area with a specified `size`.
    pub(super) fn place(&mut self, size: Size) {
        match self {
            StatefulLayoutComponent::View(view) => view.place(size),
            StatefulLayoutComponent::Tiles(tiles) => tiles.place(size),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.place(size),
        }
    }

    /// External position of a component (includes border, padding, ...) that the component
    /// asks for. It is a result of the last `measure` call.
    pub(super) fn position(&self) -> Position {
        match self {
            StatefulLayoutComponent::View(view) => view.position(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.position(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.position(),
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

    pub(super) fn children(&self) -> &[LayoutChild] {
        match self {
            StatefulLayoutComponent::View(view) => view.children(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children(),
        }
    }

    fn children_mut(&mut self) -> &mut [LayoutChild] {
        match self {
            StatefulLayoutComponent::View(view) => view.children_mut(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.children_mut(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.children_mut(),
        }
    }

    /// Layout of the component itself (without children). It is a result of the last
    /// `place` call.
    fn own_layout(&self) -> Option<&NestedLayout> {
        match self {
            StatefulLayoutComponent::View(view) => view.own_layout(),
            StatefulLayoutComponent::Tiles(tiles) => tiles.own_layout(),
            StatefulLayoutComponent::Rescaler(rescaler) => rescaler.own_layout(),
        }
    }

    /// Renders every component that is rendered to a separate texture and placed by this
    /// component or by layout components nested inside of it.
    fn render_nodes(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        for child in self.children_mut() {
            match &mut child.component {
                ChildComponent::Layout(layout) => layout.render_nodes(ctx, inputs, pts),
                ChildComponent::Node(node) => node.render(ctx, inputs, pts),
            }
        }
    }

    /// Layout of the component and everything inside of it in a form that can be rendered.
    /// `None` if the layout was not calculated yet.
    ///
    /// Textures that are used by the returned layout are added to `sources`.
    fn nested_layout<'a>(
        &'a self,
        inputs: &'a InputTextures,
        sources: &mut Vec<&'a NodeTexture>,
    ) -> Option<NestedLayout> {
        let mut layout = self.own_layout()?.clone();
        layout.children = self
            .children()
            .iter()
            .filter_map(|child| child.nested_layout(inputs, sources))
            .collect();
        Some(layout)
    }
}
