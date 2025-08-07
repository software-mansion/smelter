use anyhow::Result;
use inquire::Select;
use std::string::ToString;
use strum::{Display, EnumIter, IntoEnumIterator};

mod utils;

use crate::utils::SmelterState;

#[derive(EnumIter, Display)]
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

    // let options = all::<Action>().collect();
    let options = Action::iter().collect();

    let action = Select::new("Select option:", options).prompt()?;

    match action {
        Action::AddInput => state.register_input()?,
        _ => {} // TODO
    }

    Ok(())
}

fn main() -> Result<()> {
    run_demo()
}
