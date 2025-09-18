use smelter_render::scene;

use crate::*;

impl From<HorizontalAlign> for scene::HorizontalAlign {
    fn from(alignment: HorizontalAlign) -> Self {
        match alignment {
            HorizontalAlign::Left => scene::HorizontalAlign::Left,
            HorizontalAlign::Right => scene::HorizontalAlign::Right,
            HorizontalAlign::Justified => scene::HorizontalAlign::Justified,
            HorizontalAlign::Center => scene::HorizontalAlign::Center,
        }
    }
}

impl From<VerticalAlign> for scene::VerticalAlign {
    fn from(alignment: VerticalAlign) -> Self {
        match alignment {
            VerticalAlign::Top => scene::VerticalAlign::Top,
            VerticalAlign::Center => scene::VerticalAlign::Center,
            VerticalAlign::Bottom => scene::VerticalAlign::Bottom,
            VerticalAlign::Justified => scene::VerticalAlign::Justified,
        }
    }
}

impl TryFrom<AspectRatio> for (u32, u32) {
    type Error = TypeError;

    fn try_from(text: AspectRatio) -> Result<Self, Self::Error> {
        const ERROR_MESSAGE: &str = "Aspect ratio needs to be a string in the \"W:H\" format, where W and H are both unsigned integers.";
        let Some((v1_str, v2_str)) = text.0.split_once(':') else {
            return Err(TypeError::new(ERROR_MESSAGE));
        };
        let v1 = v1_str
            .parse::<u32>()
            .or(Err(TypeError::new(ERROR_MESSAGE)))?;
        let v2 = v2_str
            .parse::<u32>()
            .or(Err(TypeError::new(ERROR_MESSAGE)))?;
        Ok((v1, v2))
    }
}
