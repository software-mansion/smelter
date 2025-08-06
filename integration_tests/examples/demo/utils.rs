use anyhow::Result;
use enum_iterator::{Sequence, all};
use inquire::{min_length, Select, Text};
use serde_json::{json, Value};
use std::fmt::Display;

mod inputs;
mod outputs;

#[derive(Sequence)]
enum SmelterProtocol {
    Rtp,
    Whip,
    Mp4,
}

impl Display for SmelterProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Rtp => "rtp_stream",
            Self::Whip => "whip",
            Self::Mp4 => "mp4",
        };
        write!(f, "{msg}")
    }
}

enum TransportProtocol {
    Udp,
    TcpServer,
}

impl Display for TransportProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::TcpServer => "tcp_server",
            Self::Udp => "udp",
        };
        write!(f, "{msg}")
    }
}

pub struct ExampleState {
    inputs: Vec<Value>,
    outputs: Vec<Value>,
}

impl ExampleState {
    pub fn new() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![],
        }
    }

    pub fn register_input() -> Result<()> {
        let prot_opts = all::

        let protocol = Select::new("Select input protocol: ", prot_opts).prompt()?;

        Ok(())
    }
}
