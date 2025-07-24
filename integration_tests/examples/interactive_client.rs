use std::{
    env,
    fmt::{Display, Result},
};

use inquire::Select;

fn main() {
    ffmpeg_next::format::network::init();

    loop {
        single_loop();
    }
}

fn single_loop() -> Result<()> {
    match select_action_type()? {
        ActionType::AddInput => add_input(),
        ActionType::RemoveInput => todo!(),
        ActionType::AddOutput => todo!(),
        ActionType::RemoveOutput => todo!(),
        ActionType::UpdateOutput => todo!(),
    };

    Ok(())
}

fn add_input() -> Result<()> {}

//fn select_input_to_add()

#[derive(Debug, Clone, Copy)]
enum InputType {
    Whip,
    Hls,
    Mp4,
    RtpTcp,
    RtpUdp,
}

#[derive(Debug, Clone, Copy)]
enum TrackTypes {
    Audio,
    Video,
    VideoAndAudio,
}

#[derive(Debug, Clone, Copy)]
enum ActionType {
    AddInput,
    RemoveInput,
    AddOutput,
    RemoveOutput,
    UpdateOutput,
}

fn select_input_type() -> Result<TrackTypes> {
    const OPTIONS: [InputType; 4] = [
        InputType::Whip,
        InputType::Hls,
        InputType::Mp4,
        InputType::RtpTcp,
        InputType::RtpUdp,
    ];
    Ok(Select::new("Track types:", OPTIONS.to_vec()).prompt()?)
}

fn select_track_type() -> Result<TrackTypes> {
    const OPTIONS: [TrackTypes; 3] = [
        TrackTypes::VideoAndAudio,
        TrackTypes::Audio,
        TrackTypes::Video,
    ];
    Ok(Select::new("Track types:", OPTIONS.to_vec()).prompt()?)
}

fn select_action_type() -> Result<ActionType> {
    const OPTIONS: [ActionType; 5] = [
        ActionType::AddInput,
        ActionType::RemoveInput,
        ActionType::AddOutput,
        ActionType::RemoveOutput,
        ActionType::UpdateOutput,
    ];
    Ok(Select::new("Action:", OPTIONS.to_vec()).prompt()?)
}

impl Display for InputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputType::Whip => write!(f, "WHIP server (bearerToken=\"example\")"),
            InputType::Hls => write!(f, "HLS input (defaults to: HLS_)"),
            InputType::Mp4 => write!(f, "Mp4 input (defaults to: MP4_PATH)"),
            InputType::RtpTcp => write!(f, "RTP over TCP"),
            InputType::RtpUdp => write!(f, "RTP over UDP"),
        }
    }
}

impl Display for TrackTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackTypes::Audio => write!(f, "audio only"),
            TrackTypes::Video => write!(f, "video only"),
            TrackTypes::VideoAndAudio => write!(f, "video & audio"),
        }
    }
}

impl Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::AddInput => write!(f, "Add input"),
            ActionType::RemoveInput => write!(f, "Remove input"),
            ActionType::AddOutput => write!(f, "Add output"),
            ActionType::RemoveOutput => write!(f, "Remove output"),
            ActionType::UpdateOutput => write!(f, "update scene"),
        }
    }
}
