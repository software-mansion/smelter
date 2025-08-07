use anyhow::Result;
use inquire::Select;
use std::string::ToString;
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
            Action::Start => break,
            _ => {} // TODO
        }
    }

    println!("{state:?}");

    Ok(())
}

fn main() -> Result<()> {
    run_demo()
}
