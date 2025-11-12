use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    thread,
};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use serde::Serialize;
use tracing::warn;

use crate::stats::{input_reports::InputStatsReport, state::StatsState};

mod input_events;
mod input_reports;
mod input_state;
mod state;
mod utils;

pub(crate) use input_events::*;
pub(crate) use state::StatsEvent;

#[derive(Debug, Serialize, Clone)]
pub struct StatsReport {
    pub inputs: HashMap<String, InputStatsReport>,
}

pub(crate) struct StatsMonitor(Arc<Mutex<StatsState>>);

#[derive(Debug, Clone)]
pub(crate) struct StatsSender(Sender<StatsEvent>);

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
        StatsReport {
            inputs: guard
                .inputs
                .iter_mut()
                .map(|(input_ref, (_, input))| (input_ref.to_unique_string(), input.report()))
                .collect(),
        }
    }
}

impl StatsSender {
    pub fn send(&self, event: StatsEvent) {
        if let Err(TrySendError::Full(event)) = self.0.try_send(event) {
            warn!(?event, "Stats channel full")
        };
    }
}

fn run_event_loop(monitor: Weak<Mutex<StatsState>>, receiver: Receiver<StatsEvent>) {
    for event in receiver.into_iter() {
        let Some(monitor) = monitor.upgrade() else {
            return;
        };
        monitor.lock().unwrap().handle_event(event);
    }
}
