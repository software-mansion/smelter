use smelter_render::InputId;

use crate::stats::{StatsEvent, StatsSender};

use crate::prelude::*;

/// Registers an input in stats; on drop the input is marked as removed.
pub(crate) struct InputStatsGuard {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
}

impl InputStatsGuard {
    pub fn new(
        input_ref: Ref<InputId>,
        kind: InputProtocolKind,
        stats_sender: StatsSender,
    ) -> Self {
        stats_sender.send_blocking(StatsEvent::InputRegistered {
            input_ref: input_ref.clone(),
            kind,
        });
        Self {
            input_ref,
            stats_sender,
        }
    }
}

impl Drop for InputStatsGuard {
    fn drop(&mut self) {
        self.stats_sender.send_blocking(StatsEvent::InputRemoved {
            input_ref: self.input_ref.clone(),
        });
    }
}

/// Registers an output in stats; on drop the output is marked as removed.
pub(crate) struct OutputStatsGuard {
    output_ref: Ref<OutputId>,
    stats_sender: StatsSender,
}

impl OutputStatsGuard {
    pub fn new(
        output_ref: Ref<OutputId>,
        kind: OutputProtocolKind,
        stats_sender: StatsSender,
    ) -> Self {
        stats_sender.send_blocking(StatsEvent::OutputRegistered {
            output_ref: output_ref.clone(),
            kind,
        });
        Self {
            output_ref,
            stats_sender,
        }
    }
}

impl Drop for OutputStatsGuard {
    fn drop(&mut self) {
        self.stats_sender.send_blocking(StatsEvent::OutputRemoved {
            output_ref: self.output_ref.clone(),
        });
    }
}
