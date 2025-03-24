use std::ops::{Add, Div, Mul, Sub};

mod convert;
pub(crate) mod interpolation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlign {
    Left,
    Right,
    Justified,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
    Justified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RGBColor(pub u8, pub u8, pub u8);

impl RGBColor {
    pub const BLACK: Self = Self(0, 0, 0);

    pub fn to_yuv(&self) -> (f32, f32, f32) {
        let r = self.0 as f32 / 255.0;
        let g = self.1 as f32 / 255.0;
        let b = self.2 as f32 / 255.0;
        (
            ((0.299 * r) + (0.587 * g) + (0.114 * b)).clamp(0.0, 1.0),
            (((-0.168736 * r) - (0.331264 * g) + (0.5 * b)) + (128.0 / 255.0)).clamp(0.0, 1.0),
            (((0.5 * r) + (-0.418688 * g) + (-0.081312 * b)) + (128.0 / 255.0)).clamp(0.0, 1.0),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RGBAColor(pub u8, pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Degree(pub f64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsolutePosition {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub position_horizontal: HorizontalPosition,
    pub position_vertical: VerticalPosition,
    pub rotation_degrees: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerticalPosition {
    TopOffset(f32),
    BottomOffset(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HorizontalPosition {
    LeftOffset(f32),
    RightOffset(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationKind {
    Linear,
    Bounce,
    CubicBezier { x1: f64, y1: f64, x2: f64, y2: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    pub const ZERO: BorderRadius = BorderRadius {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    pub fn new_with_radius(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn clip_to_size(&self, size: Size) -> Self {
        let max_radius = f32::min(size.width, size.height) / 2.0;
        Self {
            top_left: f32::clamp(self.top_left, 0.0, max_radius),
            top_right: f32::clamp(self.top_right, 0.0, max_radius),
            bottom_right: f32::clamp(self.bottom_right, 0.0, max_radius),
            bottom_left: f32::clamp(self.bottom_left, 0.0, max_radius),
        }
    }
}

impl Mul<f32> for BorderRadius {
    type Output = BorderRadius;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            top_left: self.top_left * rhs,
            top_right: self.top_right * rhs,
            bottom_right: self.bottom_right * rhs,
            bottom_left: self.bottom_left * rhs,
        }
    }
}

impl Div<f32> for BorderRadius {
    type Output = BorderRadius;

    fn div(self, rhs: f32) -> Self::Output {
        self * (1.0 / rhs)
    }
}

impl Add<f32> for BorderRadius {
    type Output = BorderRadius;

    fn add(self, rhs: f32) -> Self::Output {
        Self {
            top_left: f32::max(self.top_left + rhs, 0.0),
            top_right: f32::max(self.top_right + rhs, 0.0),
            bottom_right: f32::max(self.bottom_right + rhs, 0.0),
            bottom_left: f32::max(self.bottom_left + rhs, 0.0),
        }
    }
}

impl Sub<f32> for BorderRadius {
    type Output = BorderRadius;

    fn sub(self, rhs: f32) -> Self::Output {
        self + (-rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: RGBAColor,
}
