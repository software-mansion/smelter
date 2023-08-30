use serde::{Deserialize, Serialize};

use crate::util::{Coord, RGBAColor};

use super::NodeId;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "transformation", rename_all = "snake_case")]
pub enum BuiltinTransformationSpec {
    TransformToResolution(TransformToResolution),
    FixedPositionLayout {
        texture_layouts: Vec<TextureLayout>,
        #[serde(default = "default_layout_background_color")]
        background_color_rgba: RGBAColor,
    },
}

impl BuiltinTransformationSpec {
    pub fn validate(&self, inputs: &Vec<NodeId>) -> Result<(), InvalidBuiltinTransformationSpec> {
        match self {
            BuiltinTransformationSpec::TransformToResolution(_) => Ok(()),
            BuiltinTransformationSpec::FixedPositionLayout {
                texture_layouts, ..
            } => {
                if texture_layouts.len() != inputs.len() {
                    return Err(
                        InvalidBuiltinTransformationSpec::InvalidTextureLayoutsCount {
                            texture_layouts_count: texture_layouts.len() as u32,
                            input_pads_count: inputs.len() as u32,
                        },
                    );
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(
    tag = "strategy",
    content = "background_color_rgba",
    rename_all = "snake_case"
)]
pub enum TransformToResolution {
    /// Rescales input in both axis to match output resolution
    Stretch,
    /// Scales input preserving aspect ratio and cuts equal parts
    /// from both sides in "sticking out" dimension
    Fill,
    /// Scales input preserving aspect ratio and
    /// fill the rest of the texture with the provided color
    Fit(RGBAColor),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TextureLayout {
    pub top: Coord,
    pub left: Coord,
    #[serde(default)]
    pub rotation: Degree,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Degree(pub i32);

fn default_layout_background_color() -> RGBAColor {
    RGBAColor(0, 0, 0, 0)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvalidBuiltinTransformationSpec {
    #[error("Texture layouts should specify TextureLayout for every input pad. Received: {texture_layouts_count}, expected: {input_pads_count}.")]
    InvalidTextureLayoutsCount {
        texture_layouts_count: u32,
        input_pads_count: u32,
    },
}
