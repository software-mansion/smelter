use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod audio_mixer;
mod decklink;
mod hls;
mod moq_client;
mod moq_server;
mod mp4;
mod raw_data;
mod rtmp;
mod rtp;
mod sync;
mod v4l2;
mod whep;
mod whip;

pub use audio_mixer::*;
pub use decklink::*;
pub use hls::*;
pub use moq_client::*;
pub use moq_server::*;
pub use mp4::*;
pub use raw_data::*;
pub use rtmp::*;
pub use rtp::*;
pub use sync::*;
pub use v4l2::*;
pub use whep::*;
pub use whip::*;

/// Stats report for inputs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputStatsReport {
    Rtp(RtpInputStatsReport),
    Whip(WhipInputStatsReport),
    Whep(WhepInputStatsReport),
    Hls(HlsInputStatsReport),
    Rtmp(RtmpInputStatsReport),
    MoqServer(MoqServerInputStatsReport),
    MoqClient(MoqClientInputStatsReport),
    Mp4(Mp4InputStatsReport),
    V4l2(V4l2InputStatsReport),
    #[serde(rename = "decklink")]
    DeckLink(DeckLinkInputStatsReport),
    RawData(RawDataInputStatsReport),
}
