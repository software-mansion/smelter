use std::{fmt::Display, sync::Arc, time::Duration};

use crate::{InputId, RendererId};

use super::{
    AbsolutePosition, BorderRadius, BoxShadow, Component, HorizontalAlign, InterpolationKind,
    RGBAColor, Size, VerticalAlign,
};

mod interpolation;
mod position;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentId(pub Arc<str>);

impl Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone)]
pub struct InputStreamComponent {
    pub id: Option<ComponentId>,
    pub input_id: InputId,
}

#[derive(Debug, Clone)]
pub struct ShaderComponent {
    pub id: Option<ComponentId>,
    pub children: Vec<Component>,

    pub shader_id: RendererId,
    pub shader_param: Option<ShaderParam>,

    pub size: Size,
}

#[derive(Debug, Clone)]
pub enum ShaderParam {
    F32(f32),
    U32(u32),
    I32(i32),
    List(Vec<ShaderParam>),
    Struct(Vec<ShaderParamStructField>),
}

#[derive(Debug, Clone)]
pub struct ShaderParamStructField {
    pub field_name: String,
    pub value: ShaderParam,
}

#[derive(Debug, Clone)]
pub struct WebViewComponent {
    pub id: Option<ComponentId>,
    pub children: Vec<Component>,

    pub instance_id: RendererId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageComponent {
    pub id: Option<ComponentId>,
    pub image_id: RendererId,
}

#[derive(Debug, Clone)]
pub struct TextComponent {
    pub id: Option<ComponentId>,
    pub text: Arc<str>,
    /// in pixels
    pub font_size: f32,
    /// in pixels, default: same as font_size
    pub line_height: f32,
    pub color: RGBAColor,
    /// https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#family-name-value
    /// use font family name, not generic family name
    pub font_family: Arc<str>,
    pub style: TextStyle,
    pub align: HorizontalAlign,
    pub weight: TextWeight,
    pub wrap: TextWrap,
    pub background_color: RGBAColor,
    pub dimensions: TextDimensions,
}

#[derive(Debug, Clone)]
pub enum TextStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone)]
pub enum TextWrap {
    None,
    Glyph,
    Word,
}

#[derive(Debug, Clone)]
pub enum TextWeight {
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

#[derive(Debug, Clone, Copy)]
pub enum TextDimensions {
    /// Renders text and "trims" texture to smallest possible size
    Fitted {
        max_width: f32,
        max_height: f32,
    },
    FittedColumn {
        width: f32,
        max_height: f32,
    },
    /// Renders text according to provided spec
    /// and outputs texture with provided fixed size
    Fixed {
        width: f32,
        height: f32,
    },
}

#[derive(Debug, Clone)]
pub struct ViewComponent {
    pub id: Option<ComponentId>,
    pub children: Vec<Component>,

    pub direction: ViewChildrenDirection,
    pub position: Position,
    pub transition: Option<Transition>,
    pub overflow: Overflow,

    pub background_color: RGBAColor,

    pub border_radius: BorderRadius,
    pub border_width: f32,
    pub border_color: RGBAColor,

    pub box_shadow: Vec<BoxShadow>,

    pub padding: Padding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Fit,
}

#[derive(Debug, Clone, Copy)]
pub struct Transition {
    pub duration: Duration,
    pub interpolation_kind: InterpolationKind,
    pub should_interrupt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    Static {
        width: Option<f32>,
        height: Option<f32>,
    },
    Absolute(AbsolutePosition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewChildrenDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const ZERO: Padding = Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone)]
pub struct RescalerComponent {
    pub id: Option<ComponentId>,
    pub child: Box<Component>,

    pub position: Position,
    pub transition: Option<Transition>,

    pub mode: RescaleMode,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,

    pub border_radius: BorderRadius,
    pub border_width: f32,
    pub border_color: RGBAColor,

    pub box_shadow: Vec<BoxShadow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescaleMode {
    Fit,
    Fill,
}

#[derive(Debug, Clone)]
pub struct TilesComponent {
    pub id: Option<ComponentId>,
    pub children: Vec<Component>,

    pub width: Option<f32>,
    pub height: Option<f32>,

    pub background_color: RGBAColor,
    pub tile_aspect_ratio: (u32, u32),
    pub margin: f32,
    pub padding: f32,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,

    pub transition: Option<Transition>,
}

impl Default for ViewComponent {
    fn default() -> Self {
        Self {
            id: None,
            children: vec![],
            direction: ViewChildrenDirection::Row,
            position: Position::Static {
                width: None,
                height: None,
            },
            transition: None,
            overflow: Overflow::Hidden,
            background_color: RGBAColor(0, 0, 0, 0),
            border_radius: BorderRadius::ZERO,
            border_width: 0.0,
            border_color: RGBAColor(0, 0, 0, 0),
            box_shadow: vec![],
            padding: Padding::ZERO,
        }
    }
}

impl Default for RescalerComponent {
    fn default() -> Self {
        Self {
            id: None,
            child: Component::View(ViewComponent::default()).into(),
            position: Position::Static {
                width: None,
                height: None,
            },
            transition: None,
            mode: RescaleMode::Fit,
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Center,
            border_radius: BorderRadius::ZERO,
            border_width: 0.0,
            border_color: RGBAColor(0, 0, 0, 0),
            box_shadow: vec![],
        }
    }
}

impl Default for TilesComponent {
    fn default() -> Self {
        Self {
            id: None,
            width: None,
            height: None,
            margin: 0.0,
            padding: 0.0,
            children: vec![],
            transition: None,
            vertical_align: VerticalAlign::Center,
            horizontal_align: HorizontalAlign::Center,
            background_color: RGBAColor(0, 0, 0, 0),
            tile_aspect_ratio: (16, 9),
        }
    }
}
