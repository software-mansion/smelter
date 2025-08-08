use std::sync::{
    atomic::{AtomicU16, Ordering},
    OnceLock,
};

use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use strum::{Display, EnumIter, IntoEnumIterator};

mod inputs;
mod outputs;

use inputs::{rtp::RtpInput, InputHandler};

use crate::utils::inputs::InputProtocol;

#[derive(Debug, EnumIter, Display)]
pub enum TransportProtocol {
    #[strum(to_string = "udp")]
    Udp,

    #[strum(to_string = "tcp_server")]
    TcpServer,
}

#[derive(Debug)]
pub struct SmelterState {
    inputs: Vec<Box<dyn InputHandler>>,
    outputs: Vec<u8>, // That is just a placeholder
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

        let input_json = input_handler.serialize();
        let input_route = format!("input/{}/register", input_handler.name());

        examples::post(&input_route, &input_json)?;

        self.inputs.push(input_handler);

        Ok(())
    }
}

fn get_free_port() -> u16 {
    static LAST_PORT: OnceLock<AtomicU16> = OnceLock::new();
    let port =
        LAST_PORT.get_or_init(|| AtomicU16::new(10_000 + (rand::random::<u16>() % 5_000) * 2));
    port.fetch_add(2, Ordering::Relaxed)
}
