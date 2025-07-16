use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for an input stream from HLS source.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsInput {
    /// URL to HLS playlist
    pub url: Arc<str>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
}
