use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod audio_mixer;
mod hls;
mod moq_client;
mod moq_server;
mod mp4;
mod rtmp;
mod rtp;
mod sync;
mod whep;
mod whip;

pub use audio_mixer::*;
pub use hls::*;
pub use moq_client::*;
pub use moq_server::*;
pub use mp4::*;
pub use rtmp::*;
pub use rtp::*;
pub use sync::*;
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
}
