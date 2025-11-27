use std::{path::Path, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Framerate, Resolution};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4l2Input {
    /// Path to the V4L2 device.
    ///
    /// Typically looks like either of:
    ///   - `/dev/video[N]`, where `[N]` is the OS-assigned device number
    ///   - `/dev/v4l/by-id/[ID]`, where `[ID]` is the unique device id
    ///   - `/dev/v4l/by-path/[PATH]`, where `[PATH]` is the PCI/USB device path
    ///
    /// While the numbers assigned in `/dev/video<N>` paths can differ depending on device
    /// detection order, the `by-id` paths are always the same for a given device, and the
    /// `by-path` paths should be the same for specific ports.
    pub path: Arc<Path>,
    /// The resolution that will be negotiated with the device.
    pub resolution: Resolution,
    /// The format that will be negotiated with the device.
    pub format: V4l2InputFormat,
    /// The framerate that will be negotiated with the device.
    ///
    /// Must by either an unsigned integer, or a string in the \"NUM/DEN\" format, where NUM
    /// and DEN are both unsigned integers.
    pub framerate: Framerate,
    /// (**default=`false`**) If input is required and frames are not processed
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum V4l2InputFormat {
    /// Interleaved YUYV 4:2:2
    Yuyv,
    /// Planar NV12 (Y/UV 4:2:0)
    Nv12,
}
