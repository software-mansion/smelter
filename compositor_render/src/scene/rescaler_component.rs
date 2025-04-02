use std::time::Duration;

use crate::transformations::layout::NestedLayout;

use super::{
    components::RescalerComponent,
    layout::StatefulLayoutComponent,
    scene_state::BuildStateTreeCtx,
    transition::{TransitionOptions, TransitionState},
    types::interpolation::ContinuousValue,
    BorderRadius, BoxShadow, Component, ComponentId, HorizontalAlign, IntermediateNode, Position,
    RGBAColor, RescaleMode, SceneError, Size, StatefulComponent, VerticalAlign,
};

mod interpolation;
mod layout;

#[derive(Debug, Clone)]
pub(super) struct StatefulRescalerComponent {
    start: Option<RescalerComponentParam>,
    end: RescalerComponentParam,
    transition: Option<TransitionState>,
    child: Box<StatefulComponent>,
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

    pub(super) fn children(&self) -> Vec<&StatefulComponent> {
        vec![&self.child]
    }

    pub(super) fn children_mut(&mut self) -> Vec<&mut StatefulComponent> {
        vec![&mut self.child]
    }

    pub(super) fn position(&self, pts: Duration) -> Position {
        let rescaler = self.transition_snapshot(pts);
        rescaler.position.with_border(rescaler.border_width)
    }

    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.end.id.as_ref()
    }

    pub(super) fn intermediate_node(&self) -> IntermediateNode {
        let children = {
            let node = self.child.intermediate_node();
            match node {
                IntermediateNode::Layout { root: _, children } => children,
                _ => vec![node],
            }
        };

        IntermediateNode::Layout {
            root: StatefulLayoutComponent::Rescaler(self.clone()),
            children,
        }
    }

    pub(super) fn layout(&mut self, size: Size, pts: Duration) -> NestedLayout {
        self.transition_snapshot(pts)
            .layout(size, &mut self.child, pts)
    }
}

impl RescalerComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let previous_state = self
            .id
            .as_ref()
            .and_then(|id| ctx.prev_state.get(id))
            .and_then(|component| match component {
                StatefulComponent::Layout(StatefulLayoutComponent::Rescaler(view_state)) => {
                    Some(view_state)
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
        let rescaler = StatefulRescalerComponent {
            start,
            end,
            transition,
            child: Box::new(Component::stateful_component(*self.child, ctx)?),
        };
        Ok(StatefulComponent::Layout(
            StatefulLayoutComponent::Rescaler(rescaler),
        ))
    }
}
