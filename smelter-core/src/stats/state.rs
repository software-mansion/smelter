use std::collections::HashMap;
use std::time::{Duration, Instant};

use smelter_render::InputId;

use crate::stats::{
    input::{AudioMixerStatsEvent, InputStatsState},
    output::OutputStatsState,
};

use crate::prelude::*;

pub(crate) struct StatsState {
    pub inputs: HashMap<Ref<InputId>, (Instant, InputStatsState)>,
    pub outputs: HashMap<Ref<OutputId>, (Instant, OutputStatsState)>,
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
    /// Protocol-agnostic event from the per-input audio mixer stage. Routed
    /// to the matching input's audio-mixer sub-state regardless of input kind.
    AudioMixer {
        input_ref: Ref<InputId>,
        event: AudioMixerStatsEvent,
    },
    Output {
        output_ref: Ref<OutputId>,
        event: OutputStatsEvent,
    },
    NewOutput {
        output_ref: Ref<OutputId>,
        kind: OutputProtocolKind,
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
            StatsEvent::AudioMixer { input_ref, event } => {
                // Audio-mixer events arrive only for inputs that have audio,
                // and they alone can't determine the protocol kind, so we
                // skip them if the input hasn't been registered with any
                // other event yet — the next protocol event will create it
                // and subsequent mixer events will land.
                if let Some((updated_at, input)) = self.inputs.get_mut(&input_ref)
                    && let Some(state) = input.audio_mixer_state_mut()
                {
                    *updated_at = now;
                    state.handle_event(event);
                }
            }
            StatsEvent::Output { output_ref, event } => {
                if !self.outputs.contains_key(&output_ref) {
                    let kind = OutputProtocolKind::from(&event);
                    self.outputs
                        .insert(output_ref.clone(), (now, OutputStatsState::new(kind)));
                }
                if let Some((updated_at, output)) = self.outputs.get_mut(&output_ref) {
                    *updated_at = now;
                    output.handle_event(event);
                }
            }
            StatsEvent::NewOutput { output_ref, kind } => {
                self.outputs
                    .insert(output_ref, (now, OutputStatsState::new(kind)));
            }
        }

        // drop inputs that did not have an update for 5 minutes
        self.inputs
            .retain(|_, (updated_at, _)| *updated_at + Duration::from_secs(300) > now);
    }
}
