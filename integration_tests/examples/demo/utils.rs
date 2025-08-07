use anyhow::Result;
use inquire::{min_length, Select, Text};
use serde_json::{json, Value};
use strum::{Display, EnumIter, IntoEnumIterator};

mod inputs;
mod outputs;

use inputs::{rtp::RtpInput, InputHandler};

use crate::utils::inputs::{mp4::Mp4Input, whip::WhipInput};

#[derive(Debug, EnumIter, Display)]
enum InputProtocol {
    #[strum(to_string = "rtp_stream")]
    Rtp,

    #[strum(to_string = "whip")]
    Whip,

    #[strum(to_string = "mp4")]
    Mp4,
}

#[derive(Debug, EnumIter, Display)]
enum TransportProtocol {
    #[strum(to_string = "udp")]
    Udp,

    #[strum(to_string = "tcp_server")]
    TcpServer,
}

#[derive(Debug)]
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
        let prot_opts = InputProtocol::iter().collect();

        let protocol = Select::new("Select input protocol:", prot_opts).prompt()?;

        let input_handler: Box<dyn InputHandler> = match protocol {
            InputProtocol::Rtp => Box::new(RtpInput::setup()?),
            InputProtocol::Whip => {
                println!("Unimplemented!");
                return Ok(());
            }
            InputProtocol::Mp4 => {
                println!("Unimplemented!");
                return Ok(());
            }
        };

        self.inputs.push(input_handler);

        Ok(())
    }
}

#[derive(Debug, Display, EnumIter, Clone)]
enum RegisterOptions {
    #[strum(to_string = "Set video stream")]
    SetVideoStream,

    #[strum(to_string = "Set audio stream")]
    SetAudioStream,

    #[strum(to_string = "Done")]
    Done,
}
