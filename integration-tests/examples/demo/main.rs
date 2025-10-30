use std::{
    env::{self, VarError},
    thread,
    time::Duration,
};

use inquire::{Confirm, InquireError, Select};
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

const IP: &str = "127.0.0.1";
const JSON_ENV: &str = "DEMO_JSON";

#[derive(Debug, EnumIter, Display, Clone, PartialEq)]
pub enum Action {
    #[strum(to_string = "Start")]
    Start,

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

    #[strum(to_string = "JSON dump")]
    JsonDump,
}

fn run_demo() {
    while let Err(error) = examples::post("reset", &json!({})) {
        error!(%error, "Initial reset failed.");
        thread::sleep(Duration::from_secs(3));
    }

    let mut options = Action::iter().collect::<Vec<_>>();
    let mut state = match SmelterState::try_read_dump_from_env(JSON_ENV) {
        Some(state) => {
            // Reading state from json starts the demo automatically,
            // so this action needs to be removed on succesful read.
            options.retain(|opt| *opt != Action::Start);
            state
        }
        None => SmelterState::new(),
    };

    loop {
        let action = Select::new("Select option:", options.clone()).prompt();
        let action = match action {
            Ok(a) => a,
            Err(error) => match error {
                InquireError::OperationInterrupted | InquireError::OperationCanceled => {
                    info!("Exit.");
                    break;
                }
                _ => {
                    error!(%error);
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
                    state = if should_reread_json_dump(JSON_ENV)
                        && let Some(new_state) = SmelterState::try_read_dump_from_env(JSON_ENV)
                    {
                        options.retain(|opt| *opt != Action::Start);
                        new_state
                    } else {
                        if !options.contains(&Action::Start) {
                            options.insert(0, Action::Start);
                        }
                        SmelterState::new()
                    };
                    Ok(())
                }
                Err(e) => Err(e.context("Reset request failed")),
            },
            Action::Start => {
                debug!("{state:#?}");
                match state.start() {
                    Ok(_) => {
                        options.retain(|a| *a != Action::Start);
                        Ok(())
                    }
                    Err(e) => Err(e.context("Start request failed")),
                }
            }
            Action::JsonDump => state.json_dump(),
        };

        if let Err(e) = action_result {
            error!("{e}");
        }
    }
}

fn should_reread_json_dump(env: &str) -> bool {
    match env::var(env) {
        Ok(_) => {
            let confirmation = Confirm::new("DEMO_JSON is set. Read the dump? [Y/n]:")
                .with_default(true)
                .prompt_skippable()
                .unwrap_or(None);
            match confirmation {
                Some(true) => true,
                Some(_) | None => false,
            }
        }
        Err(error) => match error {
            VarError::NotPresent => false,
            VarError::NotUnicode(_) => {
                error!(%error, "Error reading env.");
                false
            }
        },
    }
}

fn main() {
    let config = read_config();
    init_logger(config.logger.clone());
    run_demo()
}
