use crate::scene::{
    types::interpolation::{ContinuousValue, InterpolationState},
    BorderRadius, BoxShadow,
};

use super::{AbsolutePosition, Position};

impl ContinuousValue for Position {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        match (start, end) {
            (
                Position::Static { width, height },
                Position::Static {
                    width: width_end,
                    height: height_end,
                },
            ) => Self::Static {
                width: ContinuousValue::interpolate(width, width_end, state),
                height: ContinuousValue::interpolate(height, height_end, state),
            },
            (Position::Absolute(start), Position::Absolute(end)) => {
                Position::Absolute(ContinuousValue::interpolate(start, end, state))
            }
            (_, end) => *end,
        }
    }
}

impl ContinuousValue for AbsolutePosition {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            width: ContinuousValue::interpolate(&start.width, &end.width, state),
            height: ContinuousValue::interpolate(&start.height, &end.height, state),
            position_horizontal: ContinuousValue::interpolate(
                &start.position_horizontal,
                &end.position_horizontal,
                state,
            ),
            position_vertical: ContinuousValue::interpolate(
                &start.position_vertical,
                &end.position_vertical,
                state,
            ),
            rotation_degrees: ContinuousValue::interpolate(
                &start.rotation_degrees,
                &end.rotation_degrees,
                state,
            ),
        }
    }
}

impl ContinuousValue for BorderRadius {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            top_left: ContinuousValue::interpolate(&start.top_left, &end.top_left, state),
            top_right: ContinuousValue::interpolate(&start.top_right, &end.top_right, state),
            bottom_right: ContinuousValue::interpolate(
                &start.bottom_right,
                &end.bottom_right,
                state,
            ),
            bottom_left: ContinuousValue::interpolate(&start.bottom_left, &end.bottom_left, state),
        }
    }
}

impl ContinuousValue for Vec<BoxShadow> {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        start
            .iter()
            .zip(end.iter())
            // interpolate as long both lists have entries
            .map(|(start, end)| ContinuousValue::interpolate(start, end, state))
            // add remaining elements if end is longer
            .chain(end.iter().skip(usize::min(start.len(), end.len())).copied())
            .collect()
    }
}

impl ContinuousValue for BoxShadow {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            offset_x: ContinuousValue::interpolate(&start.offset_x, &end.offset_x, state),
            offset_y: ContinuousValue::interpolate(&start.offset_y, &end.offset_y, state),
            blur_radius: ContinuousValue::interpolate(&start.blur_radius, &end.blur_radius, state),
            color: end.color,
        }
    }
}
