use inquire::{InquireError, Select};
use integration_tests::examples;
use serde_json::json;
use smelter::{config::read_config, logger::init_logger};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, error, info};

mod autocompletion;
mod inputs;
mod outputs;
mod players;
mod smelter_state;
mod utils;

use crate::smelter_state::SmelterState;

pub const IP: &str = "127.0.0.1";

#[derive(Debug, EnumIter, Display, Clone, PartialEq)]
pub enum Action {
    #[strum(to_string = "Add input")]
    AddInput,

    #[strum(to_string = "Add output")]
    AddOutput,

    #[strum(to_string = "Remove input")]
    RemoveInput,

    #[strum(to_string = "Remove output")]
    RemoveOutput,

    #[strum(to_string = "Reorder inputs")]
    ReorderInputs,

    #[strum(to_string = "Reset")]
    Reset,

    #[strum(to_string = "Start")]
    Start,
}

fn run_demo() {
    if let Err(e) = examples::post("reset", &json!({})) {
        error!("Initial reset failed: {e}");
    }
    let mut state = SmelterState::new();

    let mut options = Action::iter().collect::<Vec<_>>();

    loop {
        let action = Select::new("Select option:", options.clone()).prompt();
        let action = match action {
            Ok(a) => a,
            Err(e) => match e {
                InquireError::OperationInterrupted | InquireError::OperationCanceled => {
                    info!("Exit.");
                    break;
                }
                _ => {
                    error!("{e}");
                    continue;
                }
            },
        };

        let action_result = match action {
            Action::AddInput => state.register_input(),
            Action::AddOutput => state.register_output(),
            Action::RemoveInput => state.unregister_input(),
            Action::RemoveOutput => state.unregister_output(),
            Action::ReorderInputs => state.reorder_inputs(),
            Action::Reset => match examples::post("reset", &json!({})) {
                Ok(_) => {
                    if !options.contains(&Action::Start) {
                        options.push(Action::Start);
                    }
                    state = SmelterState::new();
                    Ok(())
                }
                Err(e) => Err(e.context("Reset request failed")),
            },
            Action::Start => {
                debug!("{state:#?}");
                match examples::post("start", &json!({})) {
                    Ok(_) => {
                        options.retain(|a| *a != Action::Start);
                        Ok(())
                    }
                    Err(e) => Err(e.context("Start request failed")),
                }
            }
        };

        if let Err(e) = action_result {
            error!("{e}");
        }
    }
}

fn main() {
    let config = read_config();
    init_logger(config.logger.clone());
    run_demo()
}
