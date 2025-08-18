// WARN: Remove after implementing #remove
#![allow(dead_code)]
use crate::{inputs::InputHandler, players::InputPlayer};
use anyhow::Result;

#[derive(Debug)]
pub struct WhipInput {
    name: String,
}

impl WhipInput {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
        })
    }
}

impl InputHandler for WhipInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_after_registration(&mut self, _player: InputPlayer) -> Result<()> {
        Ok(())
    }
}
