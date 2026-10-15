use crate::stats::input_reports::DeckLinkInputStatsReport;

#[derive(Debug)]
pub struct DeckLinkInputState;

impl DeckLinkInputState {
    pub fn report(&mut self) -> DeckLinkInputStatsReport {
        DeckLinkInputStatsReport {}
    }
}
