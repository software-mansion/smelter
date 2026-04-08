use crate::scene::types::interpolation::{ContinuousValue, InterpolationState};

use super::RescalerComponentParam;

impl ContinuousValue for RescalerComponentParam {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            id: end.id.clone(),
            position: ContinuousValue::interpolate(&start.position, &end.position, state),
            mode: end.mode,
            horizontal_align: end.horizontal_align,
            vertical_align: end.vertical_align,
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
        }
    }
}
