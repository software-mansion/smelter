use crate::scene::Padding;

use super::{HorizontalPosition, VerticalPosition};

#[derive(Debug, Clone, Copy)]
pub struct InterpolationState(pub f64);

pub trait ContinuousValue {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self;
}

fn interpolate_f64(start: f64, end: f64, state: InterpolationState) -> f64 {
    start + ((end - start) * f64::from(state))
}

impl ContinuousValue for i32 {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        interpolate_f64(*start as f64, *end as f64, state) as Self
    }
}

impl ContinuousValue for f64 {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        interpolate_f64(*start, *end, state)
    }
}

impl ContinuousValue for f32 {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        interpolate_f64(*start as f64, *end as f64, state) as f32
    }
}

impl ContinuousValue for usize {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        interpolate_f64(*start as f64, *end as f64, state) as usize
    }
}

impl<T: ContinuousValue + Clone> ContinuousValue for Option<T> {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        match (start, end) {
            (Some(start), Some(end)) => Some(ContinuousValue::interpolate(start, end, state)),
            (_, end) => end.clone(),
        }
    }
}

impl From<InterpolationState> for f64 {
    fn from(value: InterpolationState) -> Self {
        value.0
    }
}

impl ContinuousValue for VerticalPosition {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        match (start, end) {
            (VerticalPosition::TopOffset(start), VerticalPosition::TopOffset(end)) => {
                Self::TopOffset(ContinuousValue::interpolate(start, end, state))
            }
            (VerticalPosition::BottomOffset(start), VerticalPosition::BottomOffset(end)) => {
                Self::BottomOffset(ContinuousValue::interpolate(start, end, state))
            }
            (_, end) => *end,
        }
    }
}

impl ContinuousValue for HorizontalPosition {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        match (start, end) {
            (HorizontalPosition::LeftOffset(start), HorizontalPosition::LeftOffset(end)) => {
                Self::LeftOffset(ContinuousValue::interpolate(start, end, state))
            }
            (HorizontalPosition::RightOffset(start), HorizontalPosition::RightOffset(end)) => {
                Self::RightOffset(ContinuousValue::interpolate(start, end, state))
            }
            (_, end) => *end,
        }
    }
}

impl ContinuousValue for Padding {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            top: ContinuousValue::interpolate(&start.top, &end.top, state),
            right: ContinuousValue::interpolate(&start.right, &end.right, state),
            bottom: ContinuousValue::interpolate(&start.bottom, &end.bottom, state),
            left: ContinuousValue::interpolate(&start.left, &end.left, state),
        }
    }
}
