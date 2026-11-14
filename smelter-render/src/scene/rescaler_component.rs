use std::{slice, time::Duration};

use crate::transformations::layout::NestedLayout;

use super::{
    BorderRadius, BoxShadow, Component, ComponentId, HorizontalAlign, Position, RGBAColor,
    RescaleMode, SceneError, Size, VerticalAlign,
    components::RescalerComponent,
    layout::{LayoutChild, StatefulLayoutComponent},
    scene_state::{BuildStateTreeCtx, LayoutCtx, PrevComponent},
    transition::{TransitionOptions, TransitionState},
    types::interpolation::ContinuousValue,
};

mod interpolation;
mod layout;

pub(super) struct StatefulRescalerComponent {
    start: Option<RescalerComponentParam>,
    end: RescalerComponentParam,
    transition: Option<TransitionState>,
    /// State of the component for the current frame. It is a result of the last
    /// `measure` call.
    current: RescalerComponentParam,
    /// Layout of the rescaler itself (without the child). It is a result of the last
    /// `place` call.
    layout: Option<NestedLayout>,
    child: LayoutChild,
}

#[derive(Debug, Clone, PartialEq)]
struct RescalerComponentParam {
    id: Option<ComponentId>,

    position: Position,
    mode: RescaleMode,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,

    border_radius: BorderRadius,
    border_width: f32,
    border_color: RGBAColor,

    box_shadow: Vec<BoxShadow>,
}

impl StatefulRescalerComponent {
    /// Generate state of the component for particular pts value.
    fn transition_snapshot(&self, pts: Duration) -> RescalerComponentParam {
        let (Some(transition), Some(start)) = (&self.transition, &self.start) else {
            return self.end.clone();
        };
        let interpolation_progress = transition.state(pts);
        ContinuousValue::interpolate(start, &self.end, interpolation_progress)
    }

    pub(super) fn children(&self) -> &[LayoutChild] {
        slice::from_ref(&self.child)
    }

    pub(super) fn children_mut(&mut self) -> &mut [LayoutChild] {
        slice::from_mut(&mut self.child)
    }

    /// External position of a component (includes border)
    pub(super) fn position(&self) -> Position {
        self.current.position.with_border(self.current.border_width)
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.end.id.as_ref()
    }

    pub(super) fn measure(&mut self, ctx: &LayoutCtx) {
        self.child.measure(ctx);
        self.current = self.transition_snapshot(ctx.pts);
    }

    pub(super) fn place(&mut self, size: Size) {
        self.layout = Some(self.current.place(size, &mut self.child));
    }

    pub(super) fn own_layout(&self) -> Option<&NestedLayout> {
        self.layout.as_ref()
    }
}

impl RescalerComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulLayoutComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                PrevComponent::Layout(StatefulLayoutComponent::Rescaler(rescaler_state)) => {
                    Some(rescaler_state)
                }
                _ => None,
            });

        // TODO: to handle cases like transition from top to bottom this view needs
        // to be further processed to use the same type of coordinates as end
        let start = previous_state.map(|state| state.transition_snapshot(ctx.last_render_pts));
        let end = RescalerComponentParam {
            id: self.id,
            position: self.position,
            mode: self.mode,
            horizontal_align: self.horizontal_align,
            vertical_align: self.vertical_align,
            border_radius: self.border_radius,
            border_width: self.border_width,
            border_color: self.border_color,
            box_shadow: self.box_shadow,
        };

        let props_changed = previous_state
            .map(|state| state.end != end)
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
        let mut rescaler = StatefulRescalerComponent {
            start,
            current: end.clone(),
            end,
            transition,
            layout: None,
            child: Component::layout_child(*self.child, ctx)?,
        };
        rescaler.current = rescaler.transition_snapshot(ctx.last_render_pts);
        Ok(StatefulLayoutComponent::Rescaler(rescaler))
    }
}
