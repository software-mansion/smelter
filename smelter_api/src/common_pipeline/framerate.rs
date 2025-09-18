use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum Framerate {
    String(String),
    U32(u32),
}

impl TryFrom<Framerate> for smelter_render::Framerate {
    type Error = TypeError;

    fn try_from(framerate: Framerate) -> Result<Self, Self::Error> {
        const ERROR_MESSAGE: &str = "Framerate needs to be an unsigned integer or a string in the \"NUM/DEN\" format, where NUM and DEN are both unsigned integers.";
        match framerate {
            Framerate::String(text) => {
                let Some((num_str, den_str)) = text.split_once('/') else {
                    return Err(TypeError::new(ERROR_MESSAGE));
                };
                let num = num_str
                    .parse::<u32>()
                    .or(Err(TypeError::new(ERROR_MESSAGE)))?;
                let den = den_str
                    .parse::<u32>()
                    .or(Err(TypeError::new(ERROR_MESSAGE)))?;
                Ok(smelter_render::Framerate { num, den })
            }
            Framerate::U32(num) => Ok(smelter_render::Framerate { num, den: 1 }),
        }
    }
}
