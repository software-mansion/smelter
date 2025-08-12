// WARN: Remove this after implementing #remove
#![allow(dead_code)]
use crate::utils::inputs::InputHandler;
use anyhow::Result;
use serde_json::json;

#[derive(Debug)]
pub struct Mp4Input {
    name: String,
    port: u16,
}

impl Mp4Input {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
            port: 40_000,
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

    fn serialize(&self) -> serde_json::Value {
        json!("")
    }

    fn start_ffmpeg_transmitter(&self) -> Result<()> {
        Ok(())
    }

    fn start_gstreamer_transmitter(&self) -> Result<()> {
        Ok(())
    }
}
