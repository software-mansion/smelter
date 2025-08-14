use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use serde_json::json;
use smelter::{config::read_config, logger::init_logger};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, error};

mod smelter_state;

use crate::smelter_state::SmelterState;

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

    #[strum(to_string = "Start")]
    Start,
}

fn run_demo() -> Result<()> {
    let mut state = SmelterState::new();

    let mut options = Action::iter().collect::<Vec<_>>();

    loop {
        let action = Select::new("Select option:", options.clone()).prompt();
        let action = match action {
            Ok(a) => a,
            Err(e) => {
                error!("{e}");
                break;
            }
        };

        let action_result = match action {
            Action::AddInput => state.register_input(),
            Action::AddOutput => state.register_output(),
            Action::RemoveInput => state.unregister_input(),
            Action::RemoveOutput => state.unregister_output(),
            Action::Start => {
                debug!("{state:#?}");
                options.retain(|a| *a != Action::Start);
                examples::post("start", &json!({}))?;
                Ok(())
            }
        };

        match action_result {
            Ok(_) => {}
            Err(e) => {
                error!("{e}");
                break;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let config = read_config();
    init_logger(config.logger.clone());
    run_demo()
}
