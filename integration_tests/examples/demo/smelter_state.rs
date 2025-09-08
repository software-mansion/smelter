use std::ops::Deref;
use std::ptr;

use anyhow::{Context, Result};
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

        examples::post(&input_route, &input_json)
            .with_context(|| "Input registration failed.".to_string())?;

        input_handler.on_after_registration(player)?;
        self.inputs.push(input_handler);

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)
                .with_context(|| "Output update failed".to_string())?;
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

        examples::post(&output_route, &output_json)
            .with_context(|| "Output registration failed".to_string())?;

        output_handler.on_after_registration(player)?;

        self.outputs.push(output_handler);

        Ok(())
    }

    pub fn unregister_input(&mut self) -> Result<()> {
        let input_names = self
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, i)| format!("{}. {}", idx + 1, i.name()))
            .collect::<Vec<_>>();
        if input_names.is_empty() {
            println!("No inputs to remove.");
            return Ok(());
        }
        let to_delete = Select::new("Select input to remove:", input_names).prompt()?;

        let to_delete = self.reformat_name(to_delete);

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
            examples::post(&update_route, &update_json)
                .with_context(|| "Output update failed".to_string())?;
        }

        let unregister_route = format!("input/{}/unregister", to_delete);

        examples::post(&unregister_route, &json!({}))
            .with_context(|| "Input unregistration failed".to_string())?;

        self.inputs.retain(|i| i.name() != to_delete);

        Ok(())
    }

    pub fn unregister_output(&mut self) -> Result<()> {
        let output_names = self
            .outputs
            .iter()
            .enumerate()
            .map(|(idx, o)| format!("{}. {}", idx + 1, o.name()))
            .collect::<Vec<_>>();
        if output_names.is_empty() {
            println!("No outputs to remove.");
            return Ok(());
        }

        let to_delete = Select::new("Select output to remove:", output_names).prompt()?;

        let to_delete = self.reformat_name(to_delete);

        let unregister_route = format!("output/{}/unregister", to_delete);

        examples::post(&unregister_route, &json!({}))
            .with_context(|| "Output unregistration failed".to_string())?;

        self.outputs.retain(|o| o.name() != to_delete);

        Ok(())
    }

    pub fn reorder_inputs(&mut self) -> Result<()> {
        let mut input_names = self
            .inputs
            .iter()
            .filter(|input| input.has_video())
            .enumerate()
            .map(|(idx, input)| format!("{}. {}", idx + 1, input.name()))
            .collect::<Vec<_>>();
        if input_names.len() < 2 {
            println!("Too few inputs for reorder to be possible.");
            return Ok(());
        }

        println!("Select inputs to swap places:");
        let input_name_1 = Select::new("Input 1:", input_names.clone()).prompt()?;
        input_names.retain(|input| *input != input_name_1);
        let input_name_2 = Select::new("Input 2:", input_names).prompt()?;

        let input_name_1 = self.reformat_name(input_name_1);
        let input_name_2 = self.reformat_name(input_name_2);

        let idx_1 = self
            .inputs
            .iter()
            .position(|input| input.name() == input_name_1)
            .unwrap();
        let idx_2 = self
            .inputs
            .iter()
            .position(|input| input.name() == input_name_2)
            .unwrap();

        unsafe {
            let input_1 = &mut self.inputs[idx_1] as *mut Box<dyn InputHandler>;
            let input_2 = &mut self.inputs[idx_2] as *mut Box<dyn InputHandler>;
            ptr::swap(input_1, input_2);
        }

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)?;
        }

        Ok(())
    }

    fn reformat_name(&self, input: String) -> String {
        let dot_offset = input.find(".").unwrap();
        input[dot_offset + 2..].to_string()
    }
}
