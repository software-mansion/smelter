use crate::stats::input_reports::RawDataInputStatsReport;

#[derive(Debug)]
pub struct RawDataInputState;

impl RawDataInputState {
    pub fn report(&mut self) -> RawDataInputStatsReport {
        RawDataInputStatsReport {}
    }
}
