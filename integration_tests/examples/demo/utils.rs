use std::{
    ops::Deref,
    sync::{
        atomic::{AtomicU16, Ordering},
        OnceLock,
    },
};

use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{error, warn};

mod inputs;
mod outputs;
mod players;

use inputs::{rtp::RtpInput, InputHandler};

use crate::utils::{
    inputs::InputProtocol,
    outputs::{rtp::RtpOutput, OutputHandler, OutputProtocol},
    players::{InputPlayerOptions, OutputPlayerOptions},
};

pub const IP: &str = "127.0.0.1";

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq)]
pub enum TransportProtocol {
    #[strum(to_string = "udp")]
    Udp,

    #[strum(to_string = "tcp_server")]
    TcpServer,
}

#[derive(Debug)]
pub struct SmelterState {
    inputs: Vec<Box<dyn InputHandler>>,
    outputs: Vec<Box<dyn OutputHandler>>,
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
            _ => {
                warn!("Unimplemented!");
                return Ok(());
            }
        };

        for output in &mut self.outputs {
            output.add_input(input_handler.deref());
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update();
            examples::post(&update_route, &update_json)?;
        }

        let input_json = input_handler.serialize();
        let input_route = format!("input/{}/register", input_handler.name());

        examples::post(&input_route, &input_json)?;

        let player_options = InputPlayerOptions::iter().collect::<Vec<_>>();
        loop {
            let player_choice =
                Select::new("Select transmitter:", player_options.clone()).prompt()?;
            match player_choice {
                InputPlayerOptions::StartFfmpegTransmitter => {
                    match input_handler.start_ffmpeg_transmitter() {
                        Ok(_) => break,
                        Err(e) => error!("{e}"),
                    }
                }
                InputPlayerOptions::Manual => break,
            }
        }

        self.inputs.push(input_handler);

        Ok(())
    }

    pub fn register_output(&mut self) -> Result<()> {
        let prot_opts = OutputProtocol::iter().collect();

        let protocol = Select::new("Select output protocol:", prot_opts).prompt()?;

        let mut output_handler: Box<dyn OutputHandler> = match protocol {
            OutputProtocol::Rtp => Box::new(RtpOutput::setup()?),
            _ => {
                warn!("Unimplemented!");
                return Ok(());
            }
        };

        output_handler.set_initial_scene(&self.inputs);

        let output_json = output_handler.serialize_register();
        let output_route = format!("output/{}/register", output_handler.name());

        if output_handler.transport_protocol() == TransportProtocol::TcpServer {
            examples::post(&output_route, &output_json)?;
        }

        let player_options = OutputPlayerOptions::iter().collect::<Vec<_>>();
        loop {
            let player_choice = Select::new("Select receiver", player_options.clone()).prompt()?;
            match player_choice {
                OutputPlayerOptions::StartFfmpegReceiver => {
                    match output_handler.start_ffmpeg_receiver() {
                        Ok(_) => break,
                        Err(e) => error!("{e}"),
                    }
                }
                OutputPlayerOptions::Manual => break,
            }
        }

        if output_handler.transport_protocol() == TransportProtocol::Udp {
            examples::post(&output_route, &output_json)?;
        }

        self.outputs.push(output_handler);

        Ok(())
    }

    pub fn unregister_input(&mut self) -> Result<()> {
        let to_delete = Select::new(
            "Select input to remove:",
            self.inputs.iter().clone().collect(),
        )
        .prompt()?;

        for output in &mut self.outputs {
            output.remove_input(to_delete.deref());
        }

        let unregister_route = format!("input/{}/unregister", to_delete.name());
        examples::post(&unregister_route, &json!({}))?;

        // Input to delete is chosen from existing inputs
        // so it is guaranteed that it exists in vec.
        let delete_index = self
            .inputs
            .iter()
            .position(|input| input.name() == to_delete.name())
            .unwrap();

        self.inputs.remove(delete_index);

        Ok(())
    }

    pub fn unregister_output(&mut self) -> Result<()> {
        let to_delete = Select::new(
            "Select output to remove:",
            self.outputs.iter().clone().collect(),
        )
        .prompt()?;

        let unregister_route = format!("output/{}/unregister", to_delete.name());
        examples::post(&unregister_route, &json!({}))?;

        let delete_index = self
            .outputs
            .iter()
            .position(|output| output.name() == to_delete.name())
            .unwrap();

        self.outputs.remove(delete_index);

        Ok(())
    }
}

fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port =
        LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 5_000) * 2));
    port.fetch_add(2, Ordering::Relaxed)
}
