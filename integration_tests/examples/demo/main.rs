use anyhow::Result;
use enum_iterator::{all, Sequence};
use inquire::Select;
use std::fmt::Display;

mod utils;

use crate::utils::SmelterState;

#[derive(Sequence)]
pub enum Action {
    AddInput,
    AddOutput,
    RemoveInput,
    RemoveOutput,
    Start,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Action::AddInput => "Add Input",
            Action::AddOutput => "Add Output",
            Action::RemoveInput => "Remove Input",
            Action::RemoveOutput => "Remove Output",
            Action::Start => "Start",
        };

        write!(f, "{msg}")
    }
}

fn run_demo() -> Result<()> {
    let mut state = SmelterState::new();

    let options = all::<Action>().collect();

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
