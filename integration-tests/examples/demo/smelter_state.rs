use std::env::VarError;
use std::{env, fs, mem};

use anyhow::{Context, Result};
use inquire::Select;
use integration_tests::examples;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::{debug, error};

use crate::inputs::InputHandle;
use crate::inputs::hls::HlsInputBuilder;
use crate::inputs::mp4::Mp4InputBuilder;
use crate::inputs::whep::WhepInputBuilder;
use crate::inputs::whip::WhipInputBuilder;

use crate::outputs::hls::HlsOutputBuilder;
use crate::outputs::mp4::Mp4OutputBuilder;
use crate::outputs::whep::WhepOutputBuilder;
use crate::outputs::whip::WhipOutputBuilder;
use crate::utils::parse_json;
use crate::{
    inputs::{InputProtocol, rtp::RtpInputBuilder},
    outputs::{OutputHandle, OutputProtocol, rtmp::RtmpOutputBuilder, rtp::RtpOutputBuilder},
};

pub const JSON_BASE: &str = "demo_json.json";

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransportProtocol {
    #[strum(to_string = "udp")]
    Udp,

    #[strum(to_string = "tcp_server")]
    TcpServer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum RunningState {
    Running,

    #[default]
    Idle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SmelterState {
    #[serde(skip)]
    running_state: RunningState,
    inputs: Vec<InputHandle>,
    outputs: Vec<Box<dyn OutputHandle>>,
}

impl SmelterState {
    pub fn new() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![],
            running_state: RunningState::Idle,
        }
    }

    pub fn from_json(json: serde_json::Value) -> Result<Self> {
        let mut state: Self = serde_json::from_value(json)?;
        state.start()?;

        for input in &mut state.inputs {
            input.on_before_registration()?;
            examples::post(
                &format!("input/{}/register", input.name()),
                &input.serialize_register(),
            )?;
            input.on_after_registration()?;
        }

        for output in &mut state.outputs {
            output.on_before_registration()?;
            examples::post(
                &format!("output/{}/register", output.name()),
                &output.serialize_register(&state.inputs),
            )?;
            output.on_after_registration()?;
        }

        Ok(state)
    }

    pub fn try_read_dump_from_env(env: &str) -> Option<Self> {
        match env::var(env) {
            Ok(json_path) => {
                let json_val = parse_json(json_path.into());
                match json_val {
                    Ok(json) => {
                        debug!("{json:#?}");
                        match Self::from_json(json) {
                            Ok(state) => Some(state),
                            Err(error) => {
                                error!(%error, "Failed to create state from provided JSON dump.");
                                None
                            }
                        }
                    }
                    Err(error) => {
                        error!(%error, "Failed to parse JSON.");
                        None
                    }
                }
            }
            Err(error) => match error {
                VarError::NotPresent => None,
                VarError::NotUnicode(_) => {
                    error!(%error, "Error reading env.");
                    None
                }
            },
        }
    }

    pub fn start(&mut self) -> Result<()> {
        examples::post("start", &json!({}))?;
        self.running_state = RunningState::Running;
        Ok(())
    }

    pub fn register_input(&mut self) -> Result<()> {
        let prot_opts = InputProtocol::iter().collect();

        let protocol = Select::new("Select input protocol:", prot_opts).prompt()?;

        let (mut input_handle, input_json): (InputHandle, serde_json::Value) = match protocol {
            InputProtocol::Rtp => {
                let rtp_input = RtpInputBuilder::new().prompt()?.build();
                let register_request = rtp_input.serialize_register();
                (InputHandle::Rtp(rtp_input), register_request)
            }
            InputProtocol::Whip => {
                let whip_input = WhipInputBuilder::new().prompt()?.build();
                let register_request = whip_input.serialize_register();
                (InputHandle::Whip(whip_input), register_request)
            }
            InputProtocol::Whep => {
                let whep_input = WhepInputBuilder::new().prompt()?.build();
                let register_request = whep_input.serialize_register();
                (InputHandle::Whep(whep_input), register_request)
            }
            InputProtocol::Mp4 => {
                let mp4_input = Mp4InputBuilder::new().prompt()?.build();
                let register_request = mp4_input.serialize_register();
                (InputHandle::Mp4(mp4_input), register_request)
            }
            InputProtocol::Hls => {
                let hls_input = HlsInputBuilder::new().prompt()?.build();
                let register_request = hls_input.serialize_register();
                (InputHandle::Hls(hls_input), register_request)
            }
        };

        let input_route = format!("input/{}/register", input_handle.name());

        debug!("Input register request: {input_json:#?}");

        input_handle.on_before_registration()?;

        examples::post(&input_route, &input_json)
            .with_context(|| "Input registration failed.".to_string())?;

        input_handle.on_after_registration()?;
        self.inputs.push(input_handle);

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&self.inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)
                .with_context(|| "Output update failed".to_string())?;
        }

        Ok(())
    }

    pub fn register_output(&mut self) -> Result<()> {
        let prot_opts = OutputProtocol::iter().collect();

        let protocol = Select::new("Select output protocol:", prot_opts).prompt()?;

        let (mut output_handler, output_json): (Box<dyn OutputHandle>, serde_json::Value) =
            match protocol {
                OutputProtocol::Rtp => {
                    let rtp_output = RtpOutputBuilder::new().prompt()?.build();
                    let register_request = rtp_output.serialize_register(&self.inputs);
                    (Box::new(rtp_output), register_request)
                }
                OutputProtocol::Rtmp => {
                    let rtmp_output = RtmpOutputBuilder::new().prompt()?.build();
                    let register_request = rtmp_output.serialize_register(&self.inputs);
                    (Box::new(rtmp_output), register_request)
                }
                OutputProtocol::Whip => {
                    let whip_output = WhipOutputBuilder::new().prompt()?.build();
                    let register_request = whip_output.serialize_register(&self.inputs);
                    (Box::new(whip_output), register_request)
                }
                OutputProtocol::Mp4 => {
                    let mp4_output = Mp4OutputBuilder::new().prompt()?.build();
                    let register_request = mp4_output.serialize_register(&self.inputs);
                    (Box::new(mp4_output), register_request)
                }
                OutputProtocol::Whep => {
                    let whep_output = WhepOutputBuilder::new().prompt()?.build();
                    let register_request = whep_output.serialize_register(&self.inputs);
                    (Box::new(whep_output), register_request)
                }
                OutputProtocol::Hls => {
                    let hls_output = HlsOutputBuilder::new().prompt(self.running_state)?.build();
                    let register_request = hls_output.serialize_register(&self.inputs);
                    (Box::new(hls_output), register_request)
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

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&self.inputs);
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

        for output in &mut self.outputs {
            let update_route = format!("output/{}/update", output.name());
            let update_json = output.serialize_update(&self.inputs);
            debug!("{update_json:#?}");
            examples::post(&update_route, &update_json)?;
        }

        Ok(())
    }

    pub fn json_dump(&self) -> Result<()> {
        let json = serde_json::to_value(self)?;
        Ok(fs::write(JSON_BASE, serde_json::to_string_pretty(&json)?)?)
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
