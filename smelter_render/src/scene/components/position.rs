use crate::scene::AbsolutePosition;

use super::{Padding, Position};

impl Position {
    pub(crate) fn with_border(self, border_width: f32) -> Self {
        match self {
            Position::Static { width, height } => Self::Static {
                width: width.map(|w| w + 2.0 * border_width),
                height: height.map(|h| h + 2.0 * border_width),
            },
            Position::Absolute(AbsolutePosition {
                width,
                height,
                position_horizontal,
                position_vertical,
                rotation_degrees,
            }) => Self::Absolute(AbsolutePosition {
                width: width.map(|w| w + 2.0 * border_width),
                height: height.map(|h| h + 2.0 * border_width),
                position_horizontal,
                position_vertical,
                rotation_degrees,
            }),
        }
    }

    pub(crate) fn with_padding(self, padding: Padding) -> Self {
        match self {
            Position::Static { width, height } => Self::Static {
                width: width.map(|w| w + padding.horizontal()),
                height: height.map(|h| h + padding.vertical()),
            },
            Position::Absolute(AbsolutePosition {
                width,
                height,
                position_horizontal,
                position_vertical,
                rotation_degrees,
            }) => Self::Absolute(AbsolutePosition {
                width: width.map(|w| w + padding.horizontal()),
                height: height.map(|h| h + padding.vertical()),
                position_horizontal,
                position_vertical,
                rotation_degrees,
            }),
        }
    }
}
