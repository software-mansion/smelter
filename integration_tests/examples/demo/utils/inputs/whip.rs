// WARN: Remove after implementing #remove
#![allow(dead_code)]
use crate::utils::{
    inputs::{InputHandler, InputProtocol},
    TransportProtocol,
};
use anyhow::Result;
use serde_json::json;

#[derive(Debug)]
pub struct WhipInput {
    name: String,
    port: u16,
    protocol: InputProtocol,
    transport_protocol: TransportProtocol,
}

impl WhipInput {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
            port: 40_000,
            protocol: InputProtocol::Whip,
            transport_protocol: TransportProtocol::Udp,
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

    fn transport_protocol(&self) -> TransportProtocol {
        self.transport_protocol
    }

    fn serialize(&self) -> serde_json::Value {
        json!("")
    }
}
