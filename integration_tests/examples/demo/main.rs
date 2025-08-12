use std::process::{Command, Stdio};

use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use serde_json::json;
use smelter::{config::read_config, logger::init_logger};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::error;

mod utils;

use crate::utils::SmelterState;

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

    let options = Action::iter().collect::<Vec<_>>();

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
            Action::Start => break,
        };

        match action_result {
            Ok(_) => {}
            Err(e) => {
                error!("{e}");
                break;
            }
        }
    }
    println!("{state:?}");

    examples::post("start", &json!({}))?;

    let options = Action::iter()
        .filter(|a| *a != Action::Start)
        .collect::<Vec<_>>();

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
            _ => panic!("Invalid option (unreachable)"),
        };

        match action_result {
            Ok(_) => {}
            Err(e) => {
                error!("{e}");
                break;
            }
        }
    }

    // Inquire handles Ctrl+c, after it causes to break
    // out of the loop all players are killed
    Command::new("pkill")
        .args(["ffmpeg", "ffplay"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    #[allow(unreachable_code)]
    Ok(())
}

fn main() -> Result<()> {
    let config = read_config();
    init_logger(config.logger.clone());
    run_demo()
}
