use crate::utils::inputs::{InputHandler, InputProtocol};
use anyhow::Result;
use serde_json::json;

#[derive(Debug)]
pub struct Mp4Input {
    name: String,
    port: u16,
    protocol: InputProtocol,
}

impl Mp4Input {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
            port: 40_000,
            protocol: InputProtocol::Mp4,
        })
    }
}

impl InputHandler for Mp4Input {
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
