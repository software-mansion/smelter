use crate::Resolution;

use super::Size;

impl From<Resolution> for Size {
    fn from(resolution: Resolution) -> Self {
        Self {
            width: resolution.width as f32,
            height: resolution.height as f32,
        }
    }
}

impl From<Size> for Resolution {
    fn from(size: Size) -> Self {
        Self {
            width: size.width as usize,
            height: size.height as usize,
        }
    }
}
