use std::ops::Deref;
use std::{fs, mem};

use anyhow::{bail, Context, Result};
use inquire::Select;
use integration_tests::examples;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, error};

use crate::inputs::mp4::{Mp4Input, Mp4InputBuilder};
use crate::inputs::rtp::RtpInput;
use crate::inputs::whip::{WhipInput, WhipInputBuilder};
use crate::inputs::InputHandler;

use crate::outputs::mp4::{Mp4Output, Mp4OutputBuilder};
use crate::outputs::rtmp::RtmpOutput;
use crate::outputs::rtp::RtpOutput;
use crate::outputs::whep::{WhepOutput, WhepOutputBuilder};
use crate::outputs::whip::{WhipOutput, WhipOutputBuilder};
use crate::{
    inputs::{rtp::RtpInputBuilder, InputProtocol},
    outputs::{rtmp::RtmpOutputBuilder, rtp::RtpOutputBuilder, OutputHandler, OutputProtocol},
};

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    // TODO: (@jbrs) Serialize selected player to reuse it
    pub fn from_json(json: serde_json::Value) -> Result<Self> {
        let json_inputs_option = json.get("inputs");
        let inputs = match json_inputs_option {
            Some(json_inputs_val) if json_inputs_val.is_array() => {
                let json_inputs = json_inputs_val.as_array().unwrap();
                Self::parse_json_inputs(json_inputs)?
            }
            Some(_) | None => bail!("Failed to parse inputs"),
        };

        let inputs_ref = inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        let json_outputs_option = json.get("outputs");
        let outputs = match json_outputs_option {
            Some(json_outputs_val) if json_outputs_val.is_array() => {
                let json_outputs = json_outputs_val.as_array().unwrap();
                Self::parse_json_outputs(json_outputs, &inputs_ref)?
            }
            Some(_) | None => bail!("Failed to parse outputs"),
        };

        Ok(Self { inputs, outputs })
    }

    pub fn register_input(&mut self) -> Result<()> {
        let prot_opts = InputProtocol::iter().collect();

        let protocol = Select::new("Select input protocol:", prot_opts).prompt()?;

        let (mut input_handler, input_json): (Box<dyn InputHandler>, serde_json::Value) =
            match protocol {
                InputProtocol::Rtp => {
                    let rtp_input = RtpInputBuilder::new().prompt()?.build();
                    let register_request = rtp_input.serialize_register();
                    (Box::new(rtp_input), register_request)
                }
                InputProtocol::Whip => {
                    let whip_input = WhipInputBuilder::new().prompt()?.build();
                    let register_request = whip_input.serialize_register();
                    (Box::new(whip_input), register_request)
                }
                InputProtocol::Mp4 => {
                    let mp4_input = Mp4InputBuilder::new().prompt()?.build();
                    let register_request = mp4_input.serialize_register();
                    (Box::new(mp4_input), register_request)
                }
            };

        let input_route = format!("input/{}/register", input_handler.name());

        debug!("Input register request: {input_json:#?}");

        examples::post(&input_route, &input_json)
            .with_context(|| "Input registration failed.".to_string())?;

        input_handler.on_after_registration()?;
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
        let (mut output_handler, output_json): (Box<dyn OutputHandler>, serde_json::Value) =
            match protocol {
                OutputProtocol::Rtp => {
                    let rtp_output = RtpOutputBuilder::new().prompt()?.build();
                    let register_request = rtp_output.serialize_register(&inputs);
                    (Box::new(rtp_output), register_request)
                }
                OutputProtocol::Rtmp => {
                    let rtmp_output = RtmpOutputBuilder::new().prompt()?.build();
                    let register_request = rtmp_output.serialize_register(&inputs);
                    (Box::new(rtmp_output), register_request)
                }
                OutputProtocol::Whip => {
                    let whip_output = WhipOutputBuilder::new().prompt()?.build();
                    let register_request = whip_output.serialize_register(&inputs);
                    (Box::new(whip_output), register_request)
                }
                OutputProtocol::Mp4 => {
                    let mp4_output = Mp4OutputBuilder::new().prompt()?.build();
                    let register_request = mp4_output.serialize_register(&inputs);
                    (Box::new(mp4_output), register_request)
                }
                OutputProtocol::Whep => {
                    let whep_output = WhepOutputBuilder::new().prompt()?.build();
                    let register_request = whep_output.serialize_register(&inputs);
                    (Box::new(whep_output), register_request)
                }
            };

        output_handler.on_before_registration()?;

        let output_route = format!("output/{}/register", output_handler.name());

        debug!("Output register request: {output_json:#?}");

        examples::post(&output_route, &output_json)
            .with_context(|| "Output registration failed".to_string())?;

        output_handler.on_after_registration()?;

        self.outputs.push(output_handler);

        Ok(())
    }

    pub fn unregister_input(&mut self) -> Result<()> {
        let input_names = self
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, i)| OrderedItem::new(idx, i.name()))
            .collect::<Vec<_>>();
        if input_names.is_empty() {
            println!("No inputs to remove.");
            return Ok(());
        }
        let to_delete = Select::new("Select input to remove:", input_names).prompt()?;
        self.inputs.remove(to_delete.idx);

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&inputs);
            examples::post(&update_route, &update_json)
                .with_context(|| "Output update failed".to_string())?;
        }

        let unregister_route = format!("input/{}/unregister", to_delete.name);

        examples::post(&unregister_route, &json!({}))
            .with_context(|| "Input unregistration failed".to_string())?;

        Ok(())
    }

    pub fn unregister_output(&mut self) -> Result<()> {
        let output_names = self
            .outputs
            .iter()
            .enumerate()
            .map(|(idx, o)| OrderedItem::new(idx, o.name()))
            .collect::<Vec<_>>();
        if output_names.is_empty() {
            println!("No outputs to remove.");
            return Ok(());
        }

        let to_delete = Select::new("Select output to remove:", output_names).prompt()?;
        self.outputs.remove(to_delete.idx);

        let unregister_route = format!("output/{}/unregister", to_delete.name);

        examples::post(&unregister_route, &json!({}))
            .with_context(|| "Output unregistration failed".to_string())?;

        Ok(())
    }

    pub fn reorder_inputs(&mut self) -> Result<()> {
        let mut input_names = self
            .inputs
            .iter()
            .filter(|input| input.has_video())
            .enumerate()
            .map(|(idx, input)| OrderedItem::new(idx, input.name()))
            .collect::<Vec<_>>();
        if input_names.len() < 2 {
            println!("Too few inputs for reorder to be possible.");
            return Ok(());
        }

        println!("Select inputs to swap places:");
        let input_1 = Select::new("Input 1:", input_names.clone()).prompt()?;
        input_names.retain(|input| input.name != input_1.name);
        let input_2 = Select::new("Input 2:", input_names).prompt()?;

        let idx_1 = self
            .inputs
            .iter()
            .position(|input| input.name() == input_1.name)
            .unwrap();
        let idx_2 = self
            .inputs
            .iter()
            .position(|input| input.name() == input_2.name)
            .unwrap();

        let [input_1, input_2] = self.inputs.get_disjoint_mut([idx_1, idx_2])?;
        mem::swap(input_1, input_2);

        let inputs = self.inputs.iter().map(|i| i.deref()).collect::<Vec<_>>();
        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)?;
        }

        Ok(())
    }

    pub fn json_dump(&self) -> Result<()> {
        let inputs = self
            .inputs
            .iter()
            .filter_map(|i| match i.json_dump() {
                Ok(value) => Some(value),
                Err(e) => {
                    error!("Unable to serialize input {}: {e}", i.name());
                    None
                }
            })
            .collect::<Vec<_>>();
        let outputs = self
            .outputs
            .iter()
            .filter_map(|o| match o.json_dump() {
                Ok(value) => Some(value),
                Err(e) => {
                    error!("Unable to serialize output {}: {e}", o.name());
                    None
                }
            })
            .collect::<Vec<_>>();

        let json = json!({"inputs": inputs, "outputs": outputs});
        Ok(fs::write("json_dump.json", json.to_string())?)
    }

    fn parse_json_inputs(
        json_inputs: &Vec<serde_json::Value>,
    ) -> Result<Vec<Box<dyn InputHandler>>> {
        let mut inputs: Vec<Box<dyn InputHandler>> = vec![];
        for input in json_inputs {
            let input_protocol_option = input.get("type");
            match input_protocol_option {
                Some(input_protocol_value) => {
                    let input_protocol: InputProtocol =
                        serde_json::from_value(input_protocol_value.clone())?;
                    match input_protocol {
                        InputProtocol::Mp4 => {
                            let mut mp4_input: Mp4Input = serde_json::from_value(input.clone())?;
                            examples::post(
                                &format!("input/{}/register", mp4_input.name()),
                                &mp4_input.serialize_register(),
                            )?;
                            mp4_input.on_after_registration()?;
                            inputs.push(Box::new(mp4_input));
                        }
                        InputProtocol::Whip => {
                            let mut whip_input: WhipInput = serde_json::from_value(input.clone())?;
                            examples::post(
                                &format!("input/{}/register", whip_input.name()),
                                &whip_input.serialize_register(),
                            )?;
                            whip_input.on_after_registration()?;
                            inputs.push(Box::new(whip_input));
                        }
                        InputProtocol::Rtp => {
                            let mut rtp_input: RtpInput = serde_json::from_value(input.clone())?;
                            examples::post(
                                &format!("input/{}/register", rtp_input.name()),
                                &rtp_input.serialize_register(),
                            )?;
                            rtp_input.on_after_registration()?;
                            inputs.push(Box::new(rtp_input));
                        }
                    }
                }
                None => bail!("Failed to parse input protocol"),
            }
        }
        Ok(inputs)
    }

    fn parse_json_outputs(
        json_outputs: &Vec<serde_json::Value>,
        inputs: &[&dyn InputHandler],
    ) -> Result<Vec<Box<dyn OutputHandler>>> {
        let mut outputs: Vec<Box<dyn OutputHandler>> = vec![];
        for output in json_outputs {
            let output_protocol_option = output.get("type");
            match output_protocol_option {
                Some(output_protocol_value) => {
                    let output_protocol: OutputProtocol =
                        serde_json::from_value(output_protocol_value.clone())?;
                    match output_protocol {
                        OutputProtocol::Mp4 => {
                            let mut mp4_output: Mp4Output = serde_json::from_value(output.clone())?;
                            mp4_output.on_before_registration()?;
                            examples::post(
                                &format!("output/{}/register", mp4_output.name()),
                                &mp4_output.serialize_register(inputs),
                            )?;
                            mp4_output.on_after_registration()?;
                            outputs.push(Box::new(mp4_output));
                        }
                        OutputProtocol::Whep => {
                            let mut whep_output: WhepOutput =
                                serde_json::from_value(output.clone())?;
                            whep_output.on_before_registration()?;
                            examples::post(
                                &format!("output/{}/register", whep_output.name()),
                                &whep_output.serialize_register(inputs),
                            )?;
                            whep_output.on_after_registration()?;
                            outputs.push(Box::new(whep_output));
                        }
                        OutputProtocol::Whip => {
                            let mut whip_output: WhipOutput =
                                serde_json::from_value(output.clone())?;
                            whip_output.on_before_registration()?;
                            examples::post(
                                &format!("output/{}/register", whip_output.name()),
                                &whip_output.serialize_register(inputs),
                            )?;
                            whip_output.on_after_registration()?;
                            outputs.push(Box::new(whip_output));
                        }
                        OutputProtocol::Rtp => {
                            let mut rtp_output: RtpOutput = serde_json::from_value(output.clone())?;
                            rtp_output.on_before_registration()?;
                            examples::post(
                                &format!("output/{}/register", rtp_output.name()),
                                &rtp_output.serialize_register(inputs),
                            )?;
                            rtp_output.on_after_registration()?;
                            outputs.push(Box::new(rtp_output));
                        }
                        OutputProtocol::Rtmp => {
                            let mut rtmp_output: RtmpOutput =
                                serde_json::from_value(output.clone())?;
                            rtmp_output.on_before_registration()?;
                            examples::post(
                                &format!("output/{}/register", rtmp_output.name()),
                                &rtmp_output.serialize_register(inputs),
                            )?;
                            rtmp_output.on_after_registration()?;
                            outputs.push(Box::new(rtmp_output));
                        }
                    }
                }
                None => bail!("Failed to parse output protocol"),
            }
        }
        Ok(outputs)
    }
}

#[derive(Clone)]
struct OrderedItem {
    idx: usize,
    name: String,
}

impl OrderedItem {
    fn new(idx: usize, name: &str) -> Self {
        Self {
            idx,
            name: name.to_string(),
        }
    }
}

impl std::fmt::Display for OrderedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}. {}", self.idx + 1, self.name)
    }
}
