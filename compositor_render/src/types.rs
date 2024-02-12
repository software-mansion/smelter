use std::{collections::HashMap, fmt::Display, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub struct AudioSamplesSet {
    pub samples: HashMap<InputId, Vec<AudioSamplesBatch>>,
    pub start_pts: Duration,
    pub length: Duration,
}

impl AudioSamplesSet {
    pub fn end_pts(&self) -> Duration {
        self.start_pts + self.length
    }
}

#[derive(Debug)]
pub struct OutputSamples(pub HashMap<OutputId, AudioSamplesBatch>);

#[derive(Debug, Clone)]
pub struct AudioSamplesBatch {
    pub samples: Arc<AudioSamples>,
    pub start_pts: Duration,
    pub sample_rate: u32,
}

impl AudioSamplesBatch {
    pub fn end_pts(&self) -> Duration {
        self.start_pts
            + Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }
}

#[derive(Debug, Clone)]
pub enum AudioSamples {
    Mono(Vec<i16>),
    Stereo(Vec<(i16, i16)>),
}

impl AudioSamples {
    pub fn len(&self) -> usize {
        match self {
            AudioSamples::Mono(samples) => samples.len(),
            AudioSamples::Stereo(samples) => samples.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub data: YuvData,
    pub resolution: Resolution,
    pub pts: Duration,
}

#[derive(Debug, Clone)]
pub struct YuvData {
    pub y_plane: bytes::Bytes,
    pub u_plane: bytes::Bytes,
    pub v_plane: bytes::Bytes,
}

#[derive(Debug)]
pub struct FrameSet<Id>
where
    Id: From<Arc<str>>,
{
    pub frames: HashMap<Id, Frame>,
    pub pts: Duration,
}

impl<Id> FrameSet<Id>
where
    Id: From<Arc<str>>,
{
    pub fn new(pts: Duration) -> Self {
        FrameSet {
            frames: HashMap::new(),
            pts,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Framerate {
    pub num: u32,
    pub den: u32,
}

impl Framerate {
    pub fn get_interval_duration(self) -> Duration {
        Duration::from_nanos(1_000_000_000u64 * self.den as u64 / self.num as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RendererId(pub Arc<str>);

impl Display for RendererId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InputId(pub Arc<str>);

impl Display for InputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<str>> for InputId {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct OutputId(pub Arc<str>);

impl Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<str>> for OutputId {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

pub const MAX_NODE_RESOLUTION: Resolution = Resolution {
    width: 7682,
    height: 4320,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub width: usize,
    pub height: usize,
}

impl Resolution {
    pub fn ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AudioChannels {
    Mono,
    Stereo,
}
