pub use crate::adapter::{DeviceType, VideoAdapterInfo};

use crate::parameters::{H264Profile, H265Profile};

/// The device capabilities for encoding
#[derive(Debug, Clone, Copy)]
pub struct EncodeCapabilities {
    pub h264: Option<EncodeH264Capabilities>,
    pub h265: Option<EncodeH265Capabilities>,
}

/// The device capabilities for H265 encoding.
///
/// See [`H265Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct EncodeH265Capabilities {
    pub main_profile: Option<EncodeProfileCapabilities>,
}

impl EncodeH265Capabilities {
    pub fn max_profile(&self) -> Option<H265Profile> {
        if self.main_profile.is_some() {
            Some(H265Profile::Main)
        } else {
            None
        }
    }
}

/// The device capabilities for H264 encoding.
///
/// See [`H264Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct EncodeH264Capabilities {
    pub baseline_profile: Option<EncodeProfileCapabilities>,
    pub main_profile: Option<EncodeProfileCapabilities>,
    pub high_profile: Option<EncodeProfileCapabilities>,
}

impl EncodeH264Capabilities {
    pub fn max_profile(&self) -> Option<H264Profile> {
        if self.high_profile.is_some() {
            Some(H264Profile::High)
        } else if self.main_profile.is_some() {
            Some(H264Profile::Main)
        } else if self.baseline_profile.is_some() {
            Some(H264Profile::Baseline)
        } else {
            None
        }
    }
}

/// Rate control capabilities supported by the encode profile
#[derive(Debug, Clone, Copy)]
pub struct RateControlCapabilities {
    pub vbr_supported: bool,
    pub cbr_supported: bool,
}

/// The device capabilities for encoding in a specific codec, at a specific profile
#[derive(Debug, Clone, Copy)]
pub struct EncodeProfileCapabilities {
    /// The minimum width of the coded image
    pub min_width: u32,
    /// The maximum width of the coded image
    pub max_width: u32,
    /// The minimum height of the coded image
    pub min_height: u32,
    /// The maximum height of the coded image
    pub max_height: u32,
    // The supported rate control modes
    pub rate_control: RateControlCapabilities,
    /// Maximum number of back references a P-frame can have
    pub max_references: u32,
}

/// The device capabilities for decoding
#[derive(Debug, Clone, Copy)]
pub struct DecodeCapabilities {
    pub h264: Option<DecodeH264Capabilities>,
    pub h265: Option<DecodeH265Capabilities>,
}

/// The device capabilities for H265 decoding.
///
/// See [`H265Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct DecodeH265Capabilities {
    pub main_profile: Option<DecodeH265ProfileCapabilities>,
}

/// The device capabilities for H265 decoding in a specific profile
#[derive(Debug, Clone, Copy)]
pub struct DecodeH265ProfileCapabilities {
    /// The minimum width of the coded image
    pub min_width: u32,
    /// The maximum width of the coded image
    pub max_width: u32,
    /// The minimum height of the coded image
    pub min_height: u32,
    /// The maximum height of the coded image
    pub max_height: u32,
    /// The maximum H265 level
    pub max_level_idc: u8,
}

/// The device capabilities for H264 decoding.
///
/// See [`H264Profile`] for information about what profiles are.
#[derive(Debug, Clone, Copy)]
pub struct DecodeH264Capabilities {
    pub baseline_profile: Option<DecodeH264ProfileCapabilities>,
    pub main_profile: Option<DecodeH264ProfileCapabilities>,
    pub high_profile: Option<DecodeH264ProfileCapabilities>,
}

/// The device capabilities for H264 decoding in a specific profile
#[derive(Debug, Clone, Copy)]
pub struct DecodeH264ProfileCapabilities {
    /// The minimum width of the coded image
    pub min_width: u32,
    /// The maximum width of the coded image
    pub max_width: u32,
    /// The minimum height of the coded image
    pub min_height: u32,
    /// The maximum height of the coded image
    pub max_height: u32,
    /// The maximum H264 level
    pub max_level_idc: u8,
}
