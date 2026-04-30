use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded};
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
mod state;
mod utils;

pub(crate) use input::*;
pub(crate) use output::*;
pub(crate) use state::StatsEvent;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
pub struct StatsReport {
    /// Stats for inputs.
    pub inputs: BTreeMap<String, InputStatsReport>,

    /// Stats for outputs.
    pub outputs: BTreeMap<String, OutputStatsReport>,
}

pub(crate) struct StatsMonitor {
    state: Arc<Mutex<StatsState>>,
    should_close: Arc<AtomicBool>,
}

impl StatsMonitor {
    pub fn shutdown(&self) {
        self.should_close.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StatsSender(Sender<Vec<StatsEvent>>);

impl StatsMonitor {
    pub fn new() -> (Self, StatsSender) {
        let state = Arc::new(Mutex::new(StatsState::new()));
        let should_close = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = bounded(10000);

        {
            let monitor = Arc::downgrade(&state);
            let should_close = should_close.clone();
            smelter_render::thread::ThreadRegistry::get().spawn(
                "Stats processor".to_string(),
                move || {
                    run_event_loop(monitor, receiver, should_close);
                },
            );
        }

        (
            Self {
                state,
                should_close,
            },
            StatsSender(sender),
        )
    }

    pub fn report(&self) -> StatsReport {
        let mut guard = self.state.lock().unwrap();
        StatsReport {
            inputs: guard
                .inputs
                .iter_mut()
                .map(|(input_ref, (_, input))| (input_ref.to_unique_string(), input.report()))
                .collect(),
            outputs: guard
                .outputs
                .iter_mut()
                .map(|(output_ref, (_, output))| (output_ref.to_unique_string(), output.report()))
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
}

fn run_event_loop(
    monitor: Weak<Mutex<StatsState>>,
    receiver: Receiver<Vec<StatsEvent>>,
    should_close: Arc<AtomicBool>,
) {
    loop {
        if should_close.load(Ordering::Relaxed) {
            return;
        }
        let events = match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(events) => events,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let Some(monitor) = monitor.upgrade() else {
            return;
        };
        for event in events {
            monitor.lock().unwrap().handle_event(event);
        }
    }
}
