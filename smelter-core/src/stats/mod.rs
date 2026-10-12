use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
    thread,
};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;
use utoipa::ToSchema;

use crate::stats::{
    input_reports::InputStatsReport, output_reports::OutputStatsReport, state::StatsState,
};

mod input;
mod input_reports;
mod output;
mod output_reports;
mod registration;
mod state;
mod utils;

pub(crate) use input::*;
pub(crate) use input_reports::{
    AudioMixerInputResamplerState, LiveSyncTrackState, SimpleSyncTrackState,
};
pub(crate) use output::*;
pub(crate) use registration::{InputStatsGuard, OutputStatsGuard};
pub(crate) use state::StatsEvent;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
pub struct StatsReport {
    /// Stats for inputs.
    pub inputs: BTreeMap<String, InputStatsReport>,

    /// Stats for outputs.
    pub outputs: BTreeMap<String, OutputStatsReport>,
}

pub(crate) struct StatsMonitor(Arc<Mutex<StatsState>>);

#[derive(Debug, Clone)]
pub(crate) struct StatsSender(Sender<Vec<StatsEvent>>);

impl StatsMonitor {
    pub fn new() -> (Self, StatsSender) {
        let monitor = Self(Arc::new(Mutex::new(StatsState::new())));
        let (sender, receiver) = bounded(10000);

        {
            let monitor = Arc::downgrade(&monitor.0);
            thread::Builder::new()
                .name("Stats processor".to_string())
                .spawn(move || {
                    run_event_loop(monitor, receiver);
                })
                .unwrap();
        }

        (monitor, StatsSender(sender))
    }

    pub fn report(&self) -> StatsReport {
        let mut guard = self.0.lock().unwrap();
        guard.drop_expired();
        StatsReport {
            inputs: guard
                .inputs
                .iter_mut()
                .map(|(input_ref, entry)| (input_ref.to_unique_string(), entry.state.report()))
                .collect(),
            outputs: guard
                .outputs
                .iter_mut()
                .map(|(output_ref, entry)| (output_ref.to_unique_string(), entry.state.report()))
                .collect(),
        }
    }
}

impl StatsSender {
    pub fn send(&self, events: impl IntoIterator<Item = StatsEvent>) {
        if let Err(TrySendError::Full(events)) = self.0.try_send(events.into_iter().collect()) {
            warn!(?events, "Stats channel is full.");
        }
    }

    /// Blocks when the channel is full. For events that can't be lost, e.g. registration.
    pub fn send_blocking(&self, events: impl IntoIterator<Item = StatsEvent>) {
        // Fails only when the stats processor is gone.
        let _ = self.0.send(events.into_iter().collect());
    }

    /// Sender without a receiver, events are discarded.
    #[cfg(test)]
    pub fn disconnected() -> Self {
        Self(bounded(1).0)
    }
}

fn run_event_loop(monitor: Weak<Mutex<StatsState>>, receiver: Receiver<Vec<StatsEvent>>) {
    for events in receiver.into_iter() {
        let Some(monitor) = monitor.upgrade() else {
            return;
        };
        for event in events {
            monitor.lock().unwrap().handle_event(event);
        }
    }
}
