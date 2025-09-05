use std::ops::Deref;

use anyhow::Result;
use inquire::Select;
use integration_tests::examples;
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::debug;

use crate::inputs::mp4::Mp4InputBuilder;
use crate::inputs::whip::WhipInputBuilder;
use crate::inputs::InputHandler;

use crate::outputs::mp4::Mp4OutputBuilder;
use crate::outputs::whip::WhipOutputBuilder;
use crate::players::{InputPlayer, OutputPlayer};
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

        let (mut input_handler, input_json, player): (
            Box<dyn InputHandler>,
            serde_json::Value,
            InputPlayer,
        ) = match protocol {
            InputProtocol::Rtp => {
                let (rtp_input, register_request, player) =
                    RtpInputBuilder::new().prompt()?.build();
                (Box::new(rtp_input), register_request, player)
            }
            InputProtocol::Whip => {
                let (whip_input, register_request, player) =
                    WhipInputBuilder::new().prompt()?.build();
                (Box::new(whip_input), register_request, player)
            }
            InputProtocol::Mp4 => {
                let (mp4_input, register_request, player) =
                    Mp4InputBuilder::new().prompt()?.build();
                (Box::new(mp4_input), register_request, player)
            }
        };

        let input_route = format!("input/{}/register", input_handler.name());

        debug!("Input register request: {input_json:#?}");

        let register_result = examples::post(&input_route, &input_json);
        if register_result.is_err() {
            println!();
            return Ok(());
        }

        input_handler.on_after_registration(player)?;
        self.inputs.push(input_handler);

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)?;
        }

        Ok(())
    }

    pub fn register_output(&mut self) -> Result<()> {
        let prot_opts = OutputProtocol::iter().collect();

        let protocol = Select::new("Select output protocol:", prot_opts).prompt()?;

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        let (mut output_handler, output_json, player): (
            Box<dyn OutputHandler>,
            serde_json::Value,
            OutputPlayer,
        ) = match protocol {
            OutputProtocol::Rtp => {
                let (rtp_output, register_request, player) =
                    RtpOutputBuilder::new().prompt()?.build(&inputs);
                (Box::new(rtp_output), register_request, player)
            }
            OutputProtocol::Rtmp => {
                let (rtmp_output, register_request, player) =
                    RtmpOutputBuilder::new().prompt()?.build(&inputs);
                (Box::new(rtmp_output), register_request, player)
            }
            OutputProtocol::Whip => {
                let (whip_output, register_request, player) =
                    WhipOutputBuilder::new().prompt()?.build(&inputs);
                (Box::new(whip_output), register_request, player)
            }
            OutputProtocol::Mp4 => {
                let (mp4_output, register_request, player) =
                    Mp4OutputBuilder::new().prompt()?.build(&inputs);
                (Box::new(mp4_output), register_request, player)
            }
        };

        output_handler.on_before_registration(player)?;

        let output_route = format!("output/{}/register", output_handler.name());

        debug!("Output register request: {output_json:#?}");

        let register_result = examples::post(&output_route, &output_json);
        if register_result.is_err() {
            println!();
            return Ok(());
        }

        output_handler.on_after_registration(player)?;

        self.outputs.push(output_handler);

        Ok(())
    }

    pub fn unregister_input(&mut self) -> Result<()> {
        let input_names = self
            .inputs
            .iter()
            .map(|i| i.name().to_string())
            .collect::<Vec<_>>();
        if input_names.is_empty() {
            println!("No inputs to remove.");
            return Ok(());
        }
        let to_delete = Select::new("Select input to remove:", input_names).prompt()?;

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let inputs = self
                .inputs
                .iter()
                .filter_map(|i| {
                    if i.name() == to_delete {
                        None
                    } else {
                        Some(i.deref())
                    }
                })
                .collect::<Vec<_>>();
            let update_json = output.serialize_update(&inputs);
            examples::post(&update_route, &update_json)?;
        }

        let unregister_route = format!("input/{}/unregister", to_delete);

        let unregister_result = examples::post(&unregister_route, &json!({}));
        if unregister_result.is_err() {
            println!();
            return Ok(());
        }

        self.inputs.retain(|i| i.name() != to_delete);

        Ok(())
    }

    pub fn unregister_output(&mut self) -> Result<()> {
        let output_names = self
            .outputs
            .iter()
            .map(|o| o.name().to_string())
            .collect::<Vec<_>>();
        if output_names.is_empty() {
            println!("No outputs to remove.");
            return Ok(());
        }
        let to_delete = Select::new("Select output to remove:", output_names).prompt()?;

        let unregister_route = format!("output/{}/unregister", to_delete);

        let unregister_result = examples::post(&unregister_route, &json!({}));
        if unregister_result.is_err() {
            println!();
            return Ok(());
        }

        self.outputs.retain(|o| o.name() != to_delete);

        Ok(())
    }
}
