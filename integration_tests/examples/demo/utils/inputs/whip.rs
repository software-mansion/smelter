use crate::utils::inputs::{InputHandler, InputProtocol};
use anyhow::Result;
use serde_json::json;

#[derive(Debug)]
pub struct WhipInput {
    name: String,
    port: u16,
    protocol: InputProtocol,
}

impl WhipInput {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
            port: 40_000,
            protocol: InputProtocol::Whip,
        })
    }
}

impl InputHandler for WhipInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn protocol(&self) -> InputProtocol {
        self.protocol
    }

    fn serialize(&self) -> serde_json::Value {
        json!("")
    }
}
