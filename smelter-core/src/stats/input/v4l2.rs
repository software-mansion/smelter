use crate::stats::input_reports::V4l2InputStatsReport;

#[derive(Debug)]
pub struct V4l2InputState;

impl V4l2InputState {
    pub fn report(&mut self) -> V4l2InputStatsReport {
        V4l2InputStatsReport {}
    }
}
