use std::time::Duration;

use crate::transformations::layout::NestedLayout;

use self::{
    layout::{layout_tiles, resize_tiles},
    tiles::Tile,
};

use super::{
    layout::StatefulLayoutComponent,
    scene_state::BuildStateTreeCtx,
    transition::{TransitionOptions, TransitionState},
    types::interpolation::ContinuousValue,
    Component, ComponentId, HorizontalAlign, IntermediateNode, Position, RGBAColor, SceneError,
    Size, StatefulComponent, TilesComponent, VerticalAlign,
};

mod interpolation;
mod layout;
mod tiles;

#[derive(Debug, Clone)]
pub(super) struct StatefulTilesComponent {
    /// Initial state for transition. To calculate scene at specific PTS you need
    /// to interpolate between this state and `self.component.tiles(...)`.
    start: Option<(Vec<Option<Tile>>, Size)>,

    /// Tile positions from last layout call. This field is a source for
    /// `start` value after scene update.
    last_layout: Option<(Vec<Option<Tile>>, Size)>,

    transition: Option<TransitionState>,

    component: TilesComponentParams,
    children: Vec<StatefulComponent>,
}

#[derive(Debug, Clone, PartialEq)]
struct TilesComponentParams {
    id: Option<ComponentId>,

    width: Option<f32>,
    height: Option<f32>,

    background_color: RGBAColor,
    tile_aspect_ratio: (u32, u32),
    margin: f32,
    padding: f32,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
}

impl StatefulTilesComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }

    fn tiles(&self, size: Size, pts: Duration) -> Vec<Option<Tile>> {
        let end = self.component.tiles(size, &self.children);
        let (Some((start, start_size)), Some(transition)) = (&self.start, &self.transition) else {
            return end.clone();
        };
        let start = resize_tiles(start, start_size, &size);
        let interpolation_progress = transition.state(pts);
        ContinuousValue::interpolate(&start, &end, interpolation_progress)
    }

    pub(super) fn position(&self, _pts: Duration) -> Position {
        Position::Static {
            width: self.component.width,
            height: self.component.height,
        }
    }

    pub(super) fn children(&self) -> Vec<&StatefulComponent> {
        self.children.iter().collect()
    }

    pub(super) fn children_mut(&mut self) -> Vec<&mut StatefulComponent> {
        self.children.iter_mut().collect()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        let children = self
            .children
            .iter()
            .flat_map(|component| {
                let node = component.intermediate_node();
                match node {
                    IntermediateNode::Layout { root: _, children } => children,
                    _ => vec![node],
                }
            })
            .collect();

        IntermediateNode::Layout {
            root: StatefulLayoutComponent::Tiles(self.clone()),
            children,
        }
    }

    pub(super) fn layout(&mut self, size: Size, pts: Duration) -> NestedLayout {
        let tiles = self.tiles(size, pts);
        let layout = layout_tiles(
            &tiles,
            size,
            &mut self.children,
            pts,
            self.component.background_color,
        );
        self.last_layout = Some((tiles, size));
        layout
    }
}

impl TilesComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                StatefulComponent::Layout(StatefulLayoutComponent::Tiles(tiles_state)) => {
                    Some(tiles_state)
                }
                _ => None,
            });

        let start = previous_state.and_then(|state| state.last_layout.clone());
        let component = TilesComponentParams {
            id: self.id,
            width: self.width,
            height: self.height,
            background_color: self.background_color,
            tile_aspect_ratio: self.tile_aspect_ratio,
            margin: self.margin,
            padding: self.padding,
            horizontal_align: self.horizontal_align,
            vertical_align: self.vertical_align,
        };
        let children = self
            .children
            .into_iter()
            .map(|c| Component::stateful_component(c, ctx))
            .collect::<Result<Vec<_>, _>>()?;

        let props_changed = previous_state
            .map(|state| {
                state.component != component
                    || Self::did_child_order_change(&state.children, &children)
            })
            .unwrap_or(false);
        let interrupt_previous_transition =
            self.transition.map(|t| t.should_interrupt).unwrap_or(false);
        let transition = TransitionState::new(
            self.transition.map(|transition| TransitionOptions {
                duration: transition.duration,
                interpolation_kind: transition.interpolation_kind,
            }),
            previous_state.and_then(|s| s.transition.clone()),
            props_changed,
            interrupt_previous_transition,
            ctx.last_render_pts,
        );
        let tiles = StatefulTilesComponent {
            start,
            last_layout: previous_state.and_then(|state| state.last_layout.clone()),
            component,
            transition,
            children,
        };

        Ok(StatefulComponent::Layout(StatefulLayoutComponent::Tiles(
            tiles,
        )))
    }

    fn did_child_order_change(
        previous_children: &[StatefulComponent],
        current_children: &[StatefulComponent],
    ) -> bool {
        if current_children.len() != previous_children.len() {
            return true;
        }
        previous_children
            .iter()
            .zip(current_children.iter())
            .any(|(c1, c2)| c1.component_id() != c2.component_id())
    }
}
