use std::collections::HashMap;
use std::time::{Duration, Instant};

use smelter_render::InputId;

use crate::stats::{input::InputStatsState, output::OutputStatsState};

use crate::prelude::*;

/// Removed inputs and outputs stay in the report for this long.
const REMOVED_RETENTION: Duration = Duration::from_secs(60);

pub(crate) struct StatsState {
    pub inputs: HashMap<Ref<InputId>, StatsEntry<InputStatsState>>,
    pub outputs: HashMap<Ref<OutputId>, StatsEntry<OutputStatsState>>,
}

pub(crate) struct StatsEntry<State> {
    pub state: State,
    pub removed_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) enum StatsEvent {
    InputRegistered {
        input_ref: Ref<InputId>,
        kind: InputProtocolKind,
    },
    InputRemoved {
        input_ref: Ref<InputId>,
    },
    Input {
        input_ref: Ref<InputId>,
        event: InputStatsEvent,
    },
    OutputRegistered {
        output_ref: Ref<OutputId>,
        kind: OutputProtocolKind,
    },
    OutputRemoved {
        output_ref: Ref<OutputId>,
    },
    Output {
        output_ref: Ref<OutputId>,
        event: OutputStatsEvent,
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
            outputs: HashMap::new(),
        }
    }

    pub fn handle_event(&mut self, event: StatsEvent) {
        match event {
            StatsEvent::InputRegistered { input_ref, kind } => {
                self.inputs
                    .insert(input_ref, StatsEntry::new(InputStatsState::new(kind)));
            }
            StatsEvent::InputRemoved { input_ref } => {
                if let Some(entry) = self.inputs.get_mut(&input_ref) {
                    entry.removed_at = Some(Instant::now());
                }
            }
            StatsEvent::Input { input_ref, event } => {
                if let Some(entry) = self.inputs.get_mut(&input_ref) {
                    entry.state.handle_event(event)
                }
            }
            StatsEvent::OutputRegistered { output_ref, kind } => {
                if let Some(state) = OutputStatsState::new(kind) {
                    self.outputs.insert(output_ref, StatsEntry::new(state));
                }
            }
            StatsEvent::OutputRemoved { output_ref } => {
                if let Some(entry) = self.outputs.get_mut(&output_ref) {
                    entry.removed_at = Some(Instant::now());
                }
            }
            StatsEvent::Output { output_ref, event } => {
                if let Some(entry) = self.outputs.get_mut(&output_ref) {
                    entry.state.handle_event(event);
                }
            }
        }
        self.drop_expired();
    }

    pub fn drop_expired(&mut self) {
        let now = Instant::now();
        let is_retained = |removed_at: Option<Instant>| {
            removed_at.is_none_or(|removed_at| removed_at + REMOVED_RETENTION > now)
        };
        self.inputs.retain(|_, entry| is_retained(entry.removed_at));
        self.outputs
            .retain(|_, entry| is_retained(entry.removed_at));
    }
}

impl<State> StatsEntry<State> {
    fn new(state: State) -> Self {
        Self {
            state,
            removed_at: None,
        }
    }
}
