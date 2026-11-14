use std::time::Duration;

use crate::transformations::layout::NestedLayout;

use self::{
    layout::{place_tiles, resize_tiles},
    tiles::Tile,
};

use super::{
    Component, ComponentId, HorizontalAlign, Position, RGBAColor, SceneError, Size, TilesComponent,
    VerticalAlign,
    layout::{LayoutChild, StatefulLayoutComponent},
    scene_state::{BuildStateTreeCtx, LayoutCtx, PrevComponent},
    transition::{TransitionOptions, TransitionState},
    types::interpolation::{ContinuousValue, InterpolationState},
};

mod interpolation;
mod layout;
mod tiles;

pub(super) struct StatefulTilesComponent {
    /// Initial state for transition. To calculate scene at specific PTS you need
    /// to interpolate between this state and `self.component.tiles(...)`.
    start: Option<(Vec<Option<Tile>>, Size)>,

    /// Tile positions from last `place` call. This field is a source for
    /// `start` value after scene update.
    last_layout: Option<(Vec<Option<Tile>>, Size)>,

    transition: Option<TransitionState>,

    /// State of the transition for the current frame. It is a result of the last
    /// `measure` call.
    transition_state: Option<InterpolationState>,

    /// Layout of the component itself (without children). It is a result of the last
    /// `place` call.
    layout: Option<NestedLayout>,

    component: TilesComponentParams,
    children: Vec<LayoutChild>,
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

    fn tiles(&self, size: Size) -> Vec<Option<Tile>> {
        let end = self.component.tiles(size, &self.children);
        let (Some((start, start_size)), Some(transition_state)) =
            (&self.start, self.transition_state)
        else {
            return end;
        };
        let start = resize_tiles(start, start_size, &size);
        ContinuousValue::interpolate(&start, &end, transition_state)
    }

    pub(super) fn position(&self) -> Position {
        Position::Static {
            width: self.component.width,
            height: self.component.height,
        }
    }

    pub(super) fn children(&self) -> &[LayoutChild] {
        &self.children
    }

    pub(super) fn children_mut(&mut self) -> &mut [LayoutChild] {
        &mut self.children
    }

    pub(super) fn measure(&mut self, ctx: &LayoutCtx) {
        for child in self.children.iter_mut() {
            child.measure(ctx);
        }
        self.transition_state = transition_state(&self.transition, ctx.pts);
    }

    pub(super) fn place(&mut self, size: Size) {
        let tiles = self.tiles(size);
        self.layout = Some(place_tiles(
            &tiles,
            size,
            &mut self.children,
            self.component.background_color,
        ));
        self.last_layout = Some((tiles, size));
    }

    pub(super) fn own_layout(&self) -> Option<&NestedLayout> {
        self.layout.as_ref()
    }
}

fn transition_state(
    transition: &Option<TransitionState>,
    pts: Duration,
) -> Option<InterpolationState> {
    transition.as_ref().map(|transition| transition.state(pts))
}

impl TilesComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulLayoutComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                PrevComponent::Layout(StatefulLayoutComponent::Tiles(tiles_state)) => {
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
            .map(|c| Component::layout_child(c, ctx))
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
            transition_state: transition_state(&transition, ctx.last_render_pts),
            transition,
            layout: None,
            component,
            children,
        };

        Ok(StatefulLayoutComponent::Tiles(tiles))
    }

    fn did_child_order_change(
        previous_children: &[LayoutChild],
        current_children: &[LayoutChild],
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
