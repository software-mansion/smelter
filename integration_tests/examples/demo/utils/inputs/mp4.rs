// WARN: Remove this after implementing #remove
#![allow(dead_code)]
use crate::utils::{inputs::InputHandler, TransportProtocol};
use anyhow::Result;
use serde_json::json;

#[derive(Debug)]
pub struct Mp4Input {
    name: String,
    port: u16,
    transport_protocol: TransportProtocol,
}

impl Mp4Input {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
            port: 40_000,
            transport_protocol: TransportProtocol::Udp,
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

    fn transport_protocol(&self) -> TransportProtocol {
        self.transport_protocol
    }

    fn serialize(&self) -> serde_json::Value {
        json!("")
    }

    fn start_ffmpeg_transmitter(&self) -> Result<()> {
        Ok(())
    }
}
