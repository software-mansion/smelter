use std::sync::Arc;

use smelter_render::MAX_NODE_RESOLUTION;
use smelter_render::scene;
use smelter_render::scene::BorderRadius;
use smelter_render::scene::Position;

use crate::*;

impl TryFrom<Component> for scene::Component {
    type Error = TypeError;

    fn try_from(node: Component) -> Result<Self, Self::Error> {
        match node {
            Component::InputStream(input) => Ok(Self::InputStream(input.into())),
            Component::View(view) => Ok(Self::View(view.try_into()?)),
            Component::WebView(web) => Ok(Self::WebView(web.try_into()?)),
            Component::Shader(shader) => Ok(Self::Shader(shader.try_into()?)),
            Component::Image(image) => Ok(Self::Image(image.into())),
            Component::Text(text) => Ok(Self::Text(text.try_into()?)),
            Component::Tiles(tiles) => Ok(Self::Tiles(tiles.try_into()?)),
            Component::Rescaler(rescaler) => Ok(Self::Rescaler(rescaler.try_into()?)),
        }
    }
}

impl From<InputStream> for scene::InputStreamComponent {
    fn from(input: InputStream) -> Self {
        Self {
            id: input.id.map(Into::into),
            input_id: input.input_id.into(),
        }
    }
}

impl TryFrom<View> for scene::ViewComponent {
    type Error = TypeError;

    fn try_from(view: View) -> Result<Self, Self::Error> {
        const VERTICAL_REQUIRED_MSG: &str = "\"View\" component with absolute positioning requires either \"top\" or \"bottom\" coordinate.";
        const VERTICAL_ONLY_ONE_MSG: &str = "Fields \"top\" and \"bottom\" are mutually exclusive, you can only specify one on a \"View\" component.";
        const HORIZONTAL_REQUIRED_MSG: &str =
            "Non-static \"View\" component requires either \"left\" or \"right\" coordinate.";
        const HORIZONTAL_ONLY_ONE_MSG: &str = "Fields \"left\" and \"right\" are mutually exclusive, you can only specify one on a \"View\" component.";
        let is_absolute_position = view.top.is_some()
            || view.bottom.is_some()
            || view.left.is_some()
            || view.right.is_some()
            || view.rotation.is_some();
        let position = if is_absolute_position {
            let position_vertical = match (view.top, view.bottom) {
                (Some(top), None) => scene::VerticalPosition::TopOffset(top),
                (None, Some(bottom)) => scene::VerticalPosition::BottomOffset(bottom),
                (None, None) => return Err(TypeError::new(VERTICAL_REQUIRED_MSG)),
                (Some(_), Some(_)) => return Err(TypeError::new(VERTICAL_ONLY_ONE_MSG)),
            };
            let position_horizontal = match (view.left, view.right) {
                (Some(left), None) => scene::HorizontalPosition::LeftOffset(left),
                (None, Some(right)) => scene::HorizontalPosition::RightOffset(right),
                (None, None) => return Err(TypeError::new(HORIZONTAL_REQUIRED_MSG)),
                (Some(_), Some(_)) => return Err(TypeError::new(HORIZONTAL_ONLY_ONE_MSG)),
            };
            Position::Absolute(scene::AbsolutePosition {
                width: view.width,
                height: view.height,
                position_horizontal,
                position_vertical,
                rotation_degrees: view.rotation.unwrap_or(0.0),
            })
        } else {
            Position::Static {
                width: view.width,
                height: view.height,
            }
        };
        let direction = match view.direction {
            Some(ViewDirection::Row) => scene::ViewChildrenDirection::Row,
            Some(ViewDirection::Column) => scene::ViewChildrenDirection::Column,
            None => scene::ViewChildrenDirection::Row,
        };
        let overflow = match view.overflow {
            Some(Overflow::Visible) => scene::Overflow::Visible,
            Some(Overflow::Hidden) => scene::Overflow::Hidden,
            Some(Overflow::Fit) => scene::Overflow::Fit,
            None => scene::Overflow::Hidden,
        };
        let padding = scene::Padding {
            top: view
                .padding_top
                .or(view.padding_vertical)
                .or(view.padding)
                .unwrap_or(0.0),
            bottom: view
                .padding_bottom
                .or(view.padding_vertical)
                .or(view.padding)
                .unwrap_or(0.0),
            left: view
                .padding_left
                .or(view.padding_horizontal)
                .or(view.padding)
                .unwrap_or(0.0),
            right: view
                .padding_right
                .or(view.padding_horizontal)
                .or(view.padding)
                .unwrap_or(0.0),
        };

        if padding.top < 0.0 || padding.right < 0.0 || padding.bottom < 0.0 || padding.left < 0.0 {
            return Err(TypeError::new("Padding values cannot be negative."));
        }

        Ok(Self {
            id: view.id.map(Into::into),
            children: view
                .children
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            direction,
            position,
            overflow,
            background_color: view
                .background_color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(0, 0, 0, 0)))?,
            transition: view.transition.map(TryInto::try_into).transpose()?,
            border_radius: BorderRadius::new_with_radius(view.border_radius.unwrap_or(0.0)),
            border_width: view.border_width.unwrap_or(0.0),
            border_color: view
                .border_color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(0, 0, 0, 0)))?,
            box_shadow: view
                .box_shadow
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            padding,
        })
    }
}

impl TryFrom<Rescaler> for scene::RescalerComponent {
    type Error = TypeError;

    fn try_from(rescaler: Rescaler) -> Result<Self, Self::Error> {
        const VERTICAL_REQUIRED_MSG: &str = "\"Rescaler\" component with absolute positioning requires either \"top\" or \"bottom\" coordinate.";
        const VERTICAL_ONLY_ONE_MSG: &str = "Fields \"top\" and \"bottom\" are mutually exclusive, you can only specify one on a \"Rescaler\" component.";
        const HORIZONTAL_REQUIRED_MSG: &str =
            "Non-static \"Rescaler\" component requires either \"left\" or \"right\" coordinate.";
        const HORIZONTAL_ONLY_ONE_MSG: &str = "Fields \"left\" and \"right\" are mutually exclusive, you can only specify one on a \"Rescaler\" component.";
        let is_absolute_position = rescaler.top.is_some()
            || rescaler.bottom.is_some()
            || rescaler.left.is_some()
            || rescaler.right.is_some()
            || rescaler.rotation.is_some();
        let position = if is_absolute_position {
            let position_vertical = match (rescaler.top, rescaler.bottom) {
                (Some(top), None) => scene::VerticalPosition::TopOffset(top),
                (None, Some(bottom)) => scene::VerticalPosition::BottomOffset(bottom),
                (None, None) => return Err(TypeError::new(VERTICAL_REQUIRED_MSG)),
                (Some(_), Some(_)) => return Err(TypeError::new(VERTICAL_ONLY_ONE_MSG)),
            };
            let position_horizontal = match (rescaler.left, rescaler.right) {
                (Some(left), None) => scene::HorizontalPosition::LeftOffset(left),
                (None, Some(right)) => scene::HorizontalPosition::RightOffset(right),
                (None, None) => return Err(TypeError::new(HORIZONTAL_REQUIRED_MSG)),
                (Some(_), Some(_)) => return Err(TypeError::new(HORIZONTAL_ONLY_ONE_MSG)),
            };
            Position::Absolute(scene::AbsolutePosition {
                width: rescaler.width,
                height: rescaler.height,
                position_horizontal,
                position_vertical,
                rotation_degrees: rescaler.rotation.unwrap_or(0.0),
            })
        } else {
            Position::Static {
                width: rescaler.width,
                height: rescaler.height,
            }
        };
        let mode = match rescaler.mode {
            Some(RescaleMode::Fit) => scene::RescaleMode::Fit,
            Some(RescaleMode::Fill) => scene::RescaleMode::Fill,
            None => scene::RescaleMode::Fit,
        };
        Ok(Self {
            id: rescaler.id.map(Into::into),
            child: Box::new((*rescaler.child).try_into()?),
            position,
            mode,
            horizontal_align: rescaler
                .horizontal_align
                .unwrap_or(HorizontalAlign::Center)
                .into(),
            vertical_align: rescaler
                .vertical_align
                .unwrap_or(VerticalAlign::Center)
                .into(),
            transition: rescaler.transition.map(TryInto::try_into).transpose()?,
            border_radius: BorderRadius::new_with_radius(rescaler.border_radius.unwrap_or(0.0)),
            border_width: rescaler.border_width.unwrap_or(0.0),
            border_color: rescaler
                .border_color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(0, 0, 0, 0)))?,
            box_shadow: rescaler
                .box_shadow
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<Shader> for scene::ShaderComponent {
    type Error = TypeError;

    fn try_from(shader: Shader) -> Result<Self, Self::Error> {
        let children = shader
            .children
            .unwrap_or_default()
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        #[cfg(target_arch = "wasm32")]
        if children.len() > 1 {
            return Err(TypeError::new(
                "Shader component can have only one child on web platform.",
            ));
        }

        Ok(Self {
            id: shader.id.map(Into::into),
            shader_id: shader.shader_id.into(),
            shader_param: shader.shader_param.map(Into::into),
            size: shader.resolution.into(),
            children,
        })
    }
}

impl From<ShaderParam> for scene::ShaderParam {
    fn from(param: ShaderParam) -> Self {
        fn from_struct_field(field: ShaderParamStructField) -> scene::ShaderParamStructField {
            scene::ShaderParamStructField {
                field_name: field.field_name,
                value: field.value.into(),
            }
        }
        match param {
            ShaderParam::F32(v) => scene::ShaderParam::F32(v),
            ShaderParam::U32(v) => scene::ShaderParam::U32(v),
            ShaderParam::I32(v) => scene::ShaderParam::I32(v),
            ShaderParam::List(v) => {
                scene::ShaderParam::List(v.into_iter().map(Into::into).collect())
            }
            ShaderParam::Struct(v) => {
                scene::ShaderParam::Struct(v.into_iter().map(from_struct_field).collect())
            }
        }
    }
}

impl From<Image> for scene::ImageComponent {
    fn from(image: Image) -> Self {
        Self {
            id: image.id.map(Into::into),
            image_id: image.image_id.into(),
            width: image.width,
            height: image.height,
        }
    }
}

impl TryFrom<Text> for scene::TextComponent {
    type Error = TypeError;

    fn try_from(text: Text) -> Result<Self, Self::Error> {
        let style = match text.style {
            Some(TextStyle::Normal) => scene::TextStyle::Normal,
            Some(TextStyle::Italic) => scene::TextStyle::Italic,
            Some(TextStyle::Oblique) => scene::TextStyle::Oblique,
            None => scene::TextStyle::Normal,
        };
        let wrap = match text.wrap {
            Some(TextWrapMode::None) => scene::TextWrap::None,
            Some(TextWrapMode::Word) => scene::TextWrap::Word,
            Some(TextWrapMode::Glyph) => scene::TextWrap::Glyph,
            None => scene::TextWrap::None,
        };
        let weight = match text.weight {
            Some(TextWeight::Thin) => scene::TextWeight::Thin,
            Some(TextWeight::ExtraLight) => scene::TextWeight::ExtraLight,
            Some(TextWeight::Light) => scene::TextWeight::Light,
            Some(TextWeight::Normal) => scene::TextWeight::Normal,
            Some(TextWeight::Medium) => scene::TextWeight::Medium,
            Some(TextWeight::SemiBold) => scene::TextWeight::SemiBold,
            Some(TextWeight::Bold) => scene::TextWeight::Bold,
            Some(TextWeight::ExtraBold) => scene::TextWeight::ExtraBold,
            Some(TextWeight::Black) => scene::TextWeight::Black,
            None => scene::TextWeight::Normal,
        };
        let dimensions = match (text.width, text.height, text.max_width, text.max_height) {
            (Some(width), Some(height), _, _) => scene::TextDimensions::Fixed { width, height },
            (None, Some(_), _, _) => {
                return Err(TypeError::new(
                    "\"height\" property on a Text component can only be provided if \"width\" is also defined.",
                ));
            }
            (Some(width), None, _, max_height) => scene::TextDimensions::FittedColumn {
                width,
                max_height: max_height.unwrap_or(MAX_NODE_RESOLUTION.height as f32),
            },
            (None, None, max_width, max_height) => scene::TextDimensions::Fitted {
                max_width: max_width.unwrap_or(MAX_NODE_RESOLUTION.width as f32),
                max_height: max_height.unwrap_or(MAX_NODE_RESOLUTION.height as f32),
            },
        };

        if text.font_size <= 0.0 {
            return Err(TypeError::new(
                "\"font_size\" property has to be larger than 0",
            ));
        }

        let line_height = text.line_height.unwrap_or(text.font_size);
        if line_height <= 0.0 {
            return Err(TypeError::new(
                "\"line_height\" property has to be larger than 0",
            ));
        }

        let text = Self {
            id: text.id.map(Into::into),
            text: text.text,
            font_size: text.font_size,
            dimensions,
            line_height,
            color: text
                .color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(255, 255, 255, 255)))?,
            font_family: text.font_family.unwrap_or_else(|| Arc::from("Verdana")),
            style,
            align: text.align.unwrap_or(HorizontalAlign::Left).into(),
            wrap,
            weight,
            background_color: text
                .background_color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(0, 0, 0, 0)))?,
        };
        Ok(text)
    }
}

impl TryFrom<WebView> for scene::WebViewComponent {
    type Error = TypeError;

    fn try_from(web: WebView) -> Result<Self, Self::Error> {
        Ok(Self {
            id: web.id.map(Into::into),
            children: web
                .children
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            instance_id: web.instance_id.into(),
        })
    }
}

impl TryFrom<Tiles> for scene::TilesComponent {
    type Error = TypeError;

    fn try_from(tiles: Tiles) -> Result<Self, Self::Error> {
        let result = Self {
            id: tiles.id.map(Into::into),
            children: tiles
                .children
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            width: tiles.width,
            height: tiles.height,

            background_color: tiles
                .background_color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(0, 0, 0, 0)))?,
            tile_aspect_ratio: tiles
                .tile_aspect_ratio
                .map(TryInto::try_into)
                .unwrap_or(Ok((16, 9)))?,
            margin: tiles.margin.unwrap_or(0.0),
            padding: tiles.padding.unwrap_or(0.0),
            horizontal_align: tiles
                .horizontal_align
                .unwrap_or(HorizontalAlign::Center)
                .into(),
            vertical_align: tiles.vertical_align.unwrap_or(VerticalAlign::Center).into(),
            transition: tiles.transition.map(TryInto::try_into).transpose()?,
        };
        Ok(result)
    }
}

impl TryFrom<BoxShadow> for scene::BoxShadow {
    type Error = TypeError;

    fn try_from(value: BoxShadow) -> Result<Self, Self::Error> {
        Ok(Self {
            offset_x: value.offset_x.unwrap_or(0.0),
            offset_y: value.offset_y.unwrap_or(0.0),
            blur_radius: value.blur_radius.unwrap_or(0.0),
            color: value
                .color
                .map(TryInto::try_into)
                .unwrap_or(Ok(scene::RGBAColor(255, 255, 255, 255)))?,
        })
    }
}
