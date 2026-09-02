use smelter_render::{FrameData, Resolution};

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct Frame {
    pub data: FrameData,
    pub resolution: Resolution,
    pub pts: Timestamp,
}

impl From<Frame> for smelter_render::Frame {
    fn from(frame: Frame) -> Self {
        Self {
            data: frame.data,
            resolution: frame.resolution,
            pts: frame.pts.to_duration_saturating(),
        }
    }
}

impl From<smelter_render::Frame> for Frame {
    fn from(frame: smelter_render::Frame) -> Self {
        Self {
            data: frame.data,
            resolution: frame.resolution,
            pts: frame.pts.into(),
        }
    }
}
