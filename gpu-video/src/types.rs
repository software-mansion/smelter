use std::num::NonZeroU32;

/// Pixel dimensions used by hardware video backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

#[cfg(all(feature = "quicksync", target_os = "linux"))]
impl VideoResolution {
    pub(crate) fn extent_2d(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }
}

/// Rational frame rate used by hardware video backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoFramerate {
    pub num: NonZeroU32,
    pub den: NonZeroU32,
}

impl VideoFramerate {
    pub fn new(num: u32, den: u32) -> Option<Self> {
        Some(Self { num: NonZeroU32::new(num)?, den: NonZeroU32::new(den)? })
    }
}
