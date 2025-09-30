use crate::scene::types::interpolation::{ContinuousValue, InterpolationState};

use super::ViewComponentParam;

impl ContinuousValue for ViewComponentParam {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            id: end.id.clone(),
            direction: end.direction.clone(),
            position: ContinuousValue::interpolate(&start.position, &end.position, state),
            background_color: end.background_color,
            overflow: end.overflow,
            border_radius: ContinuousValue::interpolate(
                &start.border_radius,
                &end.border_radius,
                state,
            ),
            border_width: ContinuousValue::interpolate(
                &start.border_width,
                &end.border_width,
                state,
            ),
            border_color: end.border_color,
            box_shadow: ContinuousValue::interpolate(&start.box_shadow, &end.box_shadow, state),
            padding: ContinuousValue::interpolate(&start.padding, &end.padding, state),
        }
    }
}
