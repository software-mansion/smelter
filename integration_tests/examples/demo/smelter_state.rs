use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, warn};

use crate::inputs::InputHandler;

use crate::{
    inputs::{rtp::RtpInputBuilder, InputProtocol},
    outputs::{rtmp::RtmpOutputBuilder, rtp::RtpOutputBuilder, OutputHandler, OutputProtocol},
};

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

        let (mut input_handler, input_json): (Box<dyn InputHandler>, serde_json::Value) =
            match protocol {
                InputProtocol::Rtp => {
                    let (rtp_input, register_request) = RtpInputBuilder::new().prompt()?.build();
                    (Box::new(rtp_input), register_request)
                }
                _ => {
                    warn!("Unimplemented!");
                    return Ok(());
                }
            };

        let input_route = format!("input/{}/register", input_handler.name());

        examples::post(&input_route, &input_json)?;
        input_handler.on_after_registration()?;
        self.inputs.push(input_handler);

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let inputs = self.inputs.iter().map(|i| i.name()).collect::<Vec<_>>();
            let update_json = output.serialize_update(&inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)?;
        }

        Ok(())
    }

    pub fn register_output(&mut self) -> Result<()> {
        let prot_opts = OutputProtocol::iter().collect();

        let protocol = Select::new("Select output protocol:", prot_opts).prompt()?;

        let inputs = self.inputs.iter().map(|i| i.name()).collect::<Vec<_>>();
        let (mut output_handler, output_json): (Box<dyn OutputHandler>, serde_json::Value) =
            match protocol {
                OutputProtocol::Rtp => {
                    let (rtp_output, register_request) =
                        RtpOutputBuilder::new().prompt()?.build(&inputs);
                    (Box::new(rtp_output), register_request)
                }
                OutputProtocol::Rtmp => {
                    let (rtmp_output, register_request) =
                        RtmpOutputBuilder::new().prompt()?.build(&inputs);
                    (Box::new(rtmp_output), register_request)
                }
                _ => {
                    warn!("Unimplemented!");
                    return Ok(());
                }
            };

        output_handler.on_before_registration()?;

        let output_route = format!("output/{}/register", output_handler.name());

        examples::post(&output_route, &output_json)?;

        output_handler.on_after_registration()?;

        self.outputs.push(output_handler);

        Ok(())
    }

    pub fn unregister_input(&mut self) -> Result<()> {
        let to_delete = Select::new(
            "Select input to remove:",
            self.inputs.iter().map(|i| i.name().to_string()).collect(),
        )
        .prompt()?;

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let inputs = self
                .inputs
                .iter()
                .filter_map(|i| {
                    if i.name() == to_delete {
                        None
                    } else {
                        Some(i.name())
                    }
                })
                .collect::<Vec<_>>();
            let update_json = output.serialize_update(&inputs);
            examples::post(&update_route, &update_json)?;
        }

        let unregister_route = format!("input/{}/unregister", to_delete);
        examples::post(&unregister_route, &json!({}))?;

        self.inputs.retain(|i| i.name() != to_delete);

        Ok(())
    }

    pub fn unregister_output(&mut self) -> Result<()> {
        let to_delete = Select::new(
            "Select output to remove:",
            self.outputs.iter().map(|o| o.name().to_string()).collect(),
        )
        .prompt()?;

        let unregister_route = format!("output/{}/unregister", to_delete);
        examples::post(&unregister_route, &json!({}))?;

        self.outputs.retain(|o| o.name() != to_delete);

        Ok(())
    }
}
