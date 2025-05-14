use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Capture streams from devices connected to Blackmagic DeckLink card.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckLink {
    /// Single DeckLink device can consist of multiple sub-devices. This field defines
    /// index of sub-device that should be used.
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub subdevice_index: Option<u32>,

    /// Select sub-device to use based on the display name. This is the value you see in e.g.
    /// Blackmagic Media Express app. like "DeckLink Quad HDMI Recorder (3)"
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub display_name: Option<String>,

    /// Persistent ID of a device represented by 32-bit hex number. Each DeckLink sub-device has a separate id.
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub persistent_id: Option<String>,

    /// (**default=`true`**) Enable audio support.
    pub enable_audio: Option<bool>,

    /// (**default=`false`**) If input is required and frames are not processed
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
}
