use std::collections::HashMap;
use std::time::{Duration, Instant};

use smelter_render::InputId;

use crate::stats::input_state::InputStatsState;

use crate::prelude::*;

pub(crate) struct StatsState {
    pub inputs: HashMap<Ref<InputId>, (Instant, InputStatsState)>,
}

#[derive(Debug, Clone)]
pub(crate) enum StatsEvent {
    Input {
        input_ref: Ref<InputId>,
        event: InputStatsEvent,
    },
    NewInput {
        input_ref: Ref<InputId>,
        kind: InputProtocolKind,
    },
}

impl IntoIterator for StatsEvent {
    type Item = Self;
    type IntoIter = std::array::IntoIter<Self::Item, 1>;

    fn into_iter(self) -> Self::IntoIter {
        [self].into_iter()
    }
}

impl StatsState {
    pub fn new() -> Self {
        Self {
            inputs: HashMap::new(),
        }
    }

    pub fn handle_event(&mut self, event: StatsEvent) {
        let now = Instant::now();
        match event {
            StatsEvent::Input { input_ref, event } => {
                if !self.inputs.contains_key(&input_ref) {
                    let kind = InputProtocolKind::from(&event);
                    self.inputs
                        .insert(input_ref.clone(), (now, InputStatsState::new(kind)));
                }
                if let Some((updated_at, input)) = self.inputs.get_mut(&input_ref) {
                    *updated_at = now;
                    input.handle_event(event)
                }
            }
            StatsEvent::NewInput { input_ref, kind } => {
                self.inputs
                    .insert(input_ref, (now, InputStatsState::new(kind)));
            }
        }

        // drop inputs that did not have an update for 5 minutes
        self.inputs
            .retain(|_, (updated_at, _)| *updated_at + Duration::from_secs(300) > now);
    }
}
