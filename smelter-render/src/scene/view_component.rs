use std::time::Duration;

use crate::{scene::ViewChildrenDirection, transformations::layout::NestedLayout};

use super::{
    BorderRadius, BoxShadow, Component, ComponentId, Overflow, Padding, Position, RGBAColor,
    SceneError, Size,
    components::ViewComponent,
    layout::{LayoutChild, StatefulLayoutComponent},
    scene_state::{BuildStateTreeCtx, LayoutCtx, PrevComponent},
    transition::{TransitionOptions, TransitionState},
    types::interpolation::ContinuousValue,
};

mod interpolation;
mod layout;

pub(super) struct StatefulViewComponent {
    start: Option<ViewComponentParam>,
    end: ViewComponentParam,
    transition: Option<TransitionState>,
    /// State of the component for the current frame. It is a result of the last
    /// `measure` call.
    current: ViewComponentParam,
    /// Layout of the view itself (without children). It is a result of the last `place` call.
    layout: Option<NestedLayout>,
    children: Vec<LayoutChild>,
}

#[derive(Debug, Clone, PartialEq)]
struct ViewComponentParam {
    id: Option<ComponentId>,

    direction: ViewChildrenDirection,
    position: Position,
    overflow: Overflow,

    background_color: RGBAColor,
    border_radius: BorderRadius,
    border_width: f32,
    border_color: RGBAColor,

    box_shadow: Vec<BoxShadow>,

    padding: Padding,
}

impl StatefulViewComponent {
    fn view(&self, pts: Duration) -> ViewComponentParam {
        let (Some(transition), Some(start)) = (&self.transition, &self.start) else {
            return self.end.clone();
        };
        let interpolation_progress = transition.state(pts);
        ContinuousValue::interpolate(start, &self.end, interpolation_progress)
    }

    pub(super) fn children(&self) -> &[LayoutChild] {
        &self.children
    }

    pub(super) fn children_mut(&mut self) -> &mut [LayoutChild] {
        &mut self.children
    }

    /// External position of a component (includes border and padding)
    pub(super) fn position(&self) -> Position {
        self.current
            .position
            .with_border(self.current.border_width)
            .with_padding(self.current.padding)
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.end.id.as_ref()
    }

    pub(super) fn measure(&mut self, ctx: &LayoutCtx) {
        for child in self.children.iter_mut() {
            child.measure(ctx);
        }
        self.current = self.view(ctx.pts);
    }

    pub(super) fn place(&mut self, size: Size) {
        self.layout = Some(self.current.place(size, &mut self.children));
    }

    pub(super) fn own_layout(&self) -> Option<&NestedLayout> {
        self.layout.as_ref()
    }
}

impl ViewComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulLayoutComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                PrevComponent::Layout(StatefulLayoutComponent::View(view_state)) => {
                    Some(view_state)
                }
                _ => None,
            });

        // TODO: to handle cases like transition from top to bottom this view needs
        // to be further processed to use the same type of coordinates as end
        let start = previous_state.map(|state| state.view(ctx.last_render_pts));
        let end = ViewComponentParam {
            id: self.id,
            direction: self.direction,
            position: self.position,
            background_color: self.background_color,
            overflow: self.overflow,
            border_radius: self.border_radius,
            border_width: self.border_width,
            border_color: self.border_color,
            box_shadow: self.box_shadow,
            padding: self.padding,
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
        let mut view = StatefulViewComponent {
            start,
            current: end.clone(),
            end,
            transition,
            layout: None,
            children: self
                .children
                .into_iter()
                .map(|c| Component::layout_child(c, ctx))
                .collect::<Result<_, _>>()?,
        };
        view.current = view.view(ctx.last_render_pts);
        Ok(StatefulLayoutComponent::View(view))
    }
}
