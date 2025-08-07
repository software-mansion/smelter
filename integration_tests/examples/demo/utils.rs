use anyhow::Result;
use enum_iterator::{all, Sequence};
use inquire::{min_length, Select, Text};
use serde_json::{json, Value};
use std::fmt::Display;

mod inputs;
mod outputs;

use inputs::{rtp::RtpInput, InputHandler};

use crate::utils::inputs::{mp4::Mp4Input, whip::WhipInput};

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

pub struct SmelterState {
    inputs: Vec<Box<dyn InputHandler>>,
    outputs: Vec<Value>,
}

impl SmelterState {
    pub fn new() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![],
        }
    }

    pub fn register_input(&mut self) -> Result<()> {
        let prot_opts = all::<SmelterProtocol>().collect();

        let protocol = Select::new("Select input protocol: ", prot_opts).prompt()?;

        let input_handler: Box<dyn InputHandler> = match protocol {
            SmelterProtocol::Rtp => Box::new(RtpInput::setup()?),
            SmelterProtocol::Whip => Box::new(WhipInput::setup()?), // TODO
            SmelterProtocol::Mp4 => Box::new(Mp4Input::setup()?),   // TODO
        };

        self.inputs.push(input_handler);

        Ok(())
    }
}

#[derive(Sequence, Clone)]
enum RegisterOptions {
    SetVideoStream,
    SetAudioStream,
    Done,
}

impl Display for RegisterOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::SetVideoStream => "Set video stream",
            Self::SetAudioStream => "Set audio stream",
            Self::Done => "Done",
        };
        write!(f, "{msg}")
    }
}
