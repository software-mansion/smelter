use std::{ops::Deref, time::Duration};

use crate::{scene::ViewChildrenDirection, transformations::layout::NestedLayout};

use super::{
    BorderRadius, BoxShadow, Component, ComponentId, IntermediateNode, Overflow, Padding, Position,
    RGBAColor, SceneError, Size, StatefulComponent,
    components::ViewComponent,
    layout::StatefulLayoutComponent,
    scene_state::BuildStateTreeCtx,
    transition::{TransitionOptions, TransitionState},
    types::interpolation::ContinuousValue,
};

mod interpolation;
mod layout;

#[derive(Debug, Clone)]
pub(super) struct StatefulViewComponent {
    start: Option<ViewComponentParam>,
    end: ViewComponentParam,
    transition: Option<TransitionState>,
    children: Vec<StatefulComponent>,
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

    pub(super) fn children(&self) -> Vec<&StatefulComponent> {
        self.children.iter().collect()
    }

    pub(super) fn children_mut(&mut self) -> Vec<&mut StatefulComponent> {
        self.children.iter_mut().collect()
    }

    /// External position of a component (includes border and padding)
    pub(super) fn position(&self, pts: Duration) -> Position {
        let view = self.view(pts);
        view.position
            .with_border(view.border_width)
            .with_padding(view.padding)
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.end.id.as_ref()
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
            root: StatefulLayoutComponent::View(self.clone()).into(),
            children,
        }
    }

    pub(super) fn layout(&mut self, size: Size, pts: Duration) -> NestedLayout {
        let view = self.view(pts);
        view.layout(size, &mut self.children, pts)
    }
}

impl ViewComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                StatefulComponent::Layout(boxed_layout) => match boxed_layout.deref() {
                    StatefulLayoutComponent::View(view_state) => Some(view_state),
                    _ => None,
                },
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
        let view = StatefulViewComponent {
            start,
            end,
            transition,
            children: self
                .children
                .into_iter()
                .map(|c| Component::stateful_component(c, ctx))
                .collect::<Result<_, _>>()?,
        };
        Ok(StatefulComponent::Layout(
            StatefulLayoutComponent::View(view).into(),
        ))
    }
}
