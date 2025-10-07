use std::time::Duration;

use self::{bounce::bounce_easing, cubic_bezier::cubic_bezier_easing};

use super::{InterpolationKind, types::interpolation::InterpolationState};

mod bounce;
mod cubic_bezier;

/// Similar concept to InterpolationState, but it represents a time instead.
/// Values between 0 and 1 represent transition and larger than 1 post transition.
///
/// If interpolation_kind is linear then InterpolationState and TransitionProgress
/// have the same numerical value.
#[derive(Debug, Clone, Copy)]
struct TransitionProgress(f64);

#[derive(Debug, Clone)]
pub(super) struct TransitionState {
    /// Additional offset for transition. It is non zero if you want to start
    /// a transition in the middle of the interpolation curve.
    initial_offset: (TransitionProgress, InterpolationState),

    // PTS of a first frame of transition.
    start_pts: Duration,

    /// Duration of the transition.
    duration: Duration,

    interpolation_kind: InterpolationKind,
}

pub(super) struct TransitionOptions {
    pub duration: Duration,
    pub interpolation_kind: InterpolationKind,
}

impl TransitionState {
    pub fn new(
        current_transition: Option<TransitionOptions>,
        previous_transition: Option<TransitionState>,
        component_props_changed: bool,
        interrupt_previous_transition: bool,
        last_pts: Duration,
    ) -> Option<Self> {
        match previous_transition {
            Some(previous_transition) if !previous_transition.is_finished(last_pts) => {
                match (component_props_changed, interrupt_previous_transition) {
                    (true, true) => current_transition.map(|opt| Self::from_options(opt, last_pts)),
                    _ => {
                        let remaining_duration = (previous_transition.start_pts
                            + previous_transition.duration)
                            .saturating_sub(last_pts);
                        let progress_offset = TransitionProgress(
                            1.0 - (remaining_duration.as_secs_f64()
                                / previous_transition.duration.as_secs_f64()),
                        );
                        let state_offset = previous_transition
                            .interpolation_kind
                            .state(progress_offset.0);
                        Some(Self {
                            initial_offset: (progress_offset, state_offset),
                            start_pts: last_pts,
                            duration: remaining_duration,
                            interpolation_kind: current_transition
                                .map(|t| t.interpolation_kind)
                                .unwrap_or(previous_transition.interpolation_kind),
                        })
                    }
                }
            }
            _ => match component_props_changed {
                true => current_transition.map(|opts| Self::from_options(opts, last_pts)),
                false => None,
            },
        }
    }

    fn from_options(options: TransitionOptions, start_pts: Duration) -> Self {
        Self {
            initial_offset: (TransitionProgress(0.0), InterpolationState(0.0)),
            start_pts,
            duration: options.duration,
            interpolation_kind: options.interpolation_kind,
        }
    }

    pub fn state(&self, pts: Duration) -> InterpolationState {
        // Value in range [0, 1], where 1 means end of transition.
        let progress =
            (pts.as_secs_f64() - self.start_pts.as_secs_f64()) / self.duration.as_secs_f64();
        // Value in range [initial_offset.0 , 1]. Previous progress ([0, 1]) is rescaled to fit
        // smaller range and offset is added.
        let progress = self.initial_offset.0.0 + progress * (1.0 - self.initial_offset.0.0);
        // Clamp just to handle a case where this function is called after transition is finished.
        let progress = f64::clamp(progress, 0.0, 1.0);
        // Value in range [initial_offset.1, 1] or [state(initial_offset.0), 1].
        let state = self.interpolation_kind.state(progress);
        // Value in range [0, 1].
        InterpolationState((state.0 - self.initial_offset.1.0) / (1.0 - self.initial_offset.1.0))
    }

    fn is_finished(&self, current_pts: Duration) -> bool {
        self.start_pts + self.duration <= current_pts
    }
}

impl InterpolationKind {
    fn state(&self, t: f64) -> InterpolationState {
        match self {
            InterpolationKind::Linear => InterpolationState(t),
            InterpolationKind::Bounce => InterpolationState(bounce_easing(t)),
            InterpolationKind::CubicBezier { x1, y1, x2, y2 } => {
                InterpolationState(cubic_bezier_easing(t, *x1, *y1, *x2, *y2))
            }
        }
    }
}
