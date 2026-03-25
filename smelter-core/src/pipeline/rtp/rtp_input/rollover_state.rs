#[derive(Debug, Default)]
pub(super) struct TimestampRolloverState {
    previous_timestamp: Option<u32>,
    rollover_count: usize,
}

impl TimestampRolloverState {
    pub fn timestamp(&mut self, current_timestamp: u32) -> u64 {
        let Some(previous_timestamp) = self.previous_timestamp else {
            self.previous_timestamp = Some(current_timestamp);
            return current_timestamp as u64;
        };

        let timestamp_diff = u32::abs_diff(previous_timestamp, current_timestamp);
        if timestamp_diff >= u32::MAX / 2 {
            if previous_timestamp > current_timestamp {
                self.rollover_count += 1;
            } else {
                // We received a packet from before the rollover, so we need to decrement the count
                self.rollover_count = self.rollover_count.saturating_sub(1);
            }
        }

        self.previous_timestamp = Some(current_timestamp);

        ((self.rollover_count as u64) * (u32::MAX as u64 + 1)) + current_timestamp as u64
    }
}

#[derive(Debug, Default)]
pub(super) struct SequenceNumberRollover {
    rollover_count: u64,
    last_value: Option<u16>,
}

impl SequenceNumberRollover {
    pub fn rolled_sequence_number(&mut self, sequence_number: u16) -> u64 {
        let last_value = *self.last_value.get_or_insert(sequence_number);

        let diff = u16::abs_diff(last_value, sequence_number);
        if diff >= u16::MAX / 2 {
            if last_value > sequence_number {
                self.rollover_count += 1;
            } else {
                // We received a packet from before the rollover, so we need to decrement the count
                self.rollover_count = self.rollover_count.saturating_sub(1);
            }
        }
        self.last_value = Some(sequence_number);

        (self.rollover_count * (u16::MAX as u64 + 1)) + sequence_number as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_rollover() {
        let mut rollover_state = TimestampRolloverState::default();

        let current_timestamp = 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            current_timestamp as u64
        );

        let current_timestamp = u32::MAX / 2 + 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            current_timestamp as u64
        );

        let current_timestamp = 0;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX);
        let current_timestamp = 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            2 * (u32::MAX as u64 + 1) + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(1);
        let current_timestamp = u32::MAX;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX);
        let current_timestamp = u32::MAX - 1;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );

        rollover_state.previous_timestamp = Some(u32::MAX - 1);
        let current_timestamp = u32::MAX;
        assert_eq!(
            rollover_state.timestamp(current_timestamp),
            u32::MAX as u64 + 1 + current_timestamp as u64
        );
    }
}
