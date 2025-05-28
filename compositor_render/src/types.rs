use std::{
    collections::HashMap,
    fmt::{self, Display},
    sync::Arc,
    time::Duration,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenderingMode {
    // - Leverage multiple views per texture
    // - Color blending in linear space
    GpuOptimized,
    // - Color blending in sRGB space
    CpuOptimized,
    // - Single view per texture
    // - Color blending in linear space (but requires additional processing)
    WebGl,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub data: FrameData,
    pub resolution: Resolution,
    pub pts: Duration,
}

#[derive(Debug, Clone)]
pub enum FrameData {
    PlanarYuv420(YuvPlanes),
    PlanarYuv422(YuvPlanes),
    PlanarYuv444(YuvPlanes),
    PlanarYuvJ420(YuvPlanes),
    InterleavedYuv422(bytes::Bytes),
    Rgba8UnormWgpuTexture(Arc<wgpu::Texture>),
    Nv12WgpuTexture(Arc<wgpu::Texture>),
}

#[derive(Clone)]
pub struct YuvPlanes {
    pub y_plane: bytes::Bytes,
    pub u_plane: bytes::Bytes,
    pub v_plane: bytes::Bytes,
}

impl fmt::Debug for YuvPlanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planar YUV data")
            .field("y_plane", &format!("len={}", self.y_plane.len()))
            .field("u_plane", &format!("len={}", self.u_plane.len()))
            .field("v_plane", &format!("len={}", self.v_plane.len()))
            .finish()
    }
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
    pub const ZERO: Self = Resolution {
        width: 0,
        height: 0,
    };

    pub const ONE_PIXEL: Self = Resolution {
        width: 1,
        height: 1,
    };

    pub const MIN_2X2: Self = Resolution {
        width: 2,
        height: 2,
    };

    pub fn ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

impl From<wgpu::Extent3d> for Resolution {
    fn from(value: wgpu::Extent3d) -> Self {
        Self {
            width: value.width as usize,
            height: value.height as usize,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFrameFormat {
    PlanarYuv420Bytes,
    RgbaWgpuTexture,
}
