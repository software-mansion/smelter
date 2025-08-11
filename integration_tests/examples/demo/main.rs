use anyhow::Result;
use inquire::Select;
use smelter::{config::read_config, logger::init_logger};
use strum::{Display, EnumIter, IntoEnumIterator};

mod utils;

use crate::utils::SmelterState;

#[derive(Debug, EnumIter, Display, Clone)]
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
        let action = Select::new("Select option:", options.clone()).prompt()?;

        match action {
            Action::AddInput => state.register_input()?,
            Action::AddOutput => state.register_output()?,
            Action::RemoveInput => state.unregister_input()?,
            Action::Start => break,
            _ => {} // TODO
        }
    }

    println!("{state:?}");

    Ok(())
}

fn main() -> Result<()> {
    let config = read_config();
    init_logger(config.logger.clone());
    run_demo()
}
