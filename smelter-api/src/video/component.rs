use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Component {
    InputStream(InputStream),
    View(View),
    WebView(WebView),
    Shader(Shader),
    Image(Image),
    Text(Text),
    Tiles(Tiles),
    Rescaler(Rescaler),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputStream {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// Id of an input. It identifies a stream registered using the
    /// `POST /api/input/{input_id}/register` request.
    pub input_id: InputId,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// List of component's children.
    #[schema(no_recursion)]
    pub children: Option<Vec<Component>>,

    /// Width of a component in pixels (without a border). Exact behavior might be different
    /// based on the parent component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub width: Option<f32>,
    /// Height of a component in pixels (without a border). Exact behavior might be different
    /// based on the parent component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub height: Option<f32>,

    /// Direction defines how static children are positioned inside a View component.
    pub direction: Option<ViewDirection>,

    /// Distance in pixels between this component's top edge and its parent's top edge (including a
    /// border). If this field is defined, then the component will ignore a layout defined by its
    /// parent.
    pub top: Option<f32>,
    /// Distance in pixels between this component's left edge and its parent's left edge (including
    /// a border). If this field is defined, this element will be absolutely positioned, instead of
    /// being laid out by its parent.
    pub left: Option<f32>,
    /// Distance in pixels between the bottom edge of this component and the bottom edge of its
    /// parent (including a border). If this field is defined, this element will be absolutely
    /// positioned, instead of being laid out by its parent.
    pub bottom: Option<f32>,
    /// Distance in pixels between this component's right edge and its parent's right edge.
    /// If this field is defined, this element will be absolutely positioned, instead of being
    /// laid out by its parent.
    pub right: Option<f32>,
    /// Rotation of a component in degrees. If this field is defined, this element will be
    /// absolutely positioned, instead of being laid out by its parent.
    pub rotation: Option<f32>,

    /// Defines how this component will behave during a scene update. This will only have an
    /// effect if the previous scene already contained a `View` component with the same id.
    pub transition: Option<Transition>,

    /// Controls what happens to content that is too big to fit into an area.
    ///
    /// Defaults to `"hidden"`.
    pub overflow: Option<Overflow>,

    /// Background color in a `"#RRGGBBAA"` format. Defaults to `"#00000000"`.
    pub background_color: Option<RGBAColor>,

    /// Radius of a rounded corner. Defaults to `0.0`.
    pub border_radius: Option<f32>,

    /// Border width. Defaults to `0.0`.
    pub border_width: Option<f32>,

    /// Border color in a `"#RRGGBBAA"` format. Defaults to `"#00000000"`.
    pub border_color: Option<RGBAColor>,

    /// List of box shadows.
    pub box_shadow: Option<Vec<BoxShadow>>,

    /// Padding for all sides of the component. Defaults to `0.0`.
    pub padding: Option<f32>,

    /// Padding for the top and bottom of the component. Defaults to `0.0`.
    pub padding_vertical: Option<f32>,

    /// Padding for the left and right of the component. Defaults to `0.0`.
    pub padding_horizontal: Option<f32>,

    /// Padding on top side in pixels. Defaults to `0.0`.
    pub padding_top: Option<f32>,

    /// Padding on right side in pixels. Defaults to `0.0`.
    pub padding_right: Option<f32>,

    /// Padding on bottom side in pixels. Defaults to `0.0`.
    pub padding_bottom: Option<f32>,

    /// Padding on left side in pixels. Defaults to `0.0`.
    pub padding_left: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BoxShadow {
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub color: Option<RGBAColor>,
    pub blur_radius: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    /// Render everything, including content that extends beyond their parent.
    Visible,
    /// Render only parts of the children that are inside their parent area.
    Hidden,
    /// If children components are too big to fit inside the parent, resize everything inside to
    /// fit.
    ///
    /// Components that have unknown sizes will be treated as if they had a size 0 when calculating
    /// scaling factor.
    ///
    /// :::warning
    /// This will resize everything inside, even absolutely positioned elements. For example, if you
    /// have an element in the bottom right corner and the content will be rescaled by a factor
    /// 0.5x, then that component will end up in the middle of its parent
    /// :::
    Fit,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ViewDirection {
    /// Children positioned from left to right.
    Row,
    /// Children positioned from top to bottom.
    Column,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rescaler {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// List of component's children.
    #[schema(no_recursion)]
    pub child: Box<Component>,

    /// Resize mode. Defaults to `"fit"`.
    pub mode: Option<RescaleMode>,
    /// Horizontal alignment. Defaults to `"center"`.
    pub horizontal_align: Option<HorizontalAlign>,
    /// Vertical alignment. Defaults to `"center"`.
    pub vertical_align: Option<VerticalAlign>,

    /// Width of a component in pixels (without a border). Exact behavior might be different
    /// based on the parent component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub width: Option<f32>,
    /// Height of a component in pixels (without a border). Exact behavior might be different
    /// based on the parent component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub height: Option<f32>,

    /// Distance in pixels between this component's top edge and its parent's top edge (including a
    /// border). If this field is defined, then the component will ignore a layout defined by its
    /// parent.
    pub top: Option<f32>,
    /// Distance in pixels between this component's left edge and its parent's left edge (including
    /// a border). If this field is defined, this element will be absolutely positioned, instead of
    /// being laid out by its parent.
    pub left: Option<f32>,
    /// Distance in pixels between the bottom edge of this component and the bottom edge of its
    /// parent (including a border). If this field is defined, this element will be absolutely
    /// positioned, instead of being laid out by its parent.
    pub bottom: Option<f32>,
    /// Distance in pixels between this component's right edge and its parent's right edge.
    /// If this field is defined, this element will be absolutely positioned, instead of being
    /// laid out by its parent.
    pub right: Option<f32>,
    /// Rotation of a component in degrees. If this field is defined, this element will be
    /// absolutely positioned, instead of being laid out by its parent.
    pub rotation: Option<f32>,

    /// Defines how this component will behave during a scene update. This will only have an
    /// effect if the previous scene already contained a `Rescaler` component with the same id.
    pub transition: Option<Transition>,

    /// Radius of a rounded corner. Defaults to `0.0`.
    pub border_radius: Option<f32>,

    /// Border width. Defaults to `0.0`.
    pub border_width: Option<f32>,

    /// Border color in a `"#RRGGBBAA"` format. Defaults to `"#00000000"`.
    pub border_color: Option<RGBAColor>,

    /// List of box shadows.
    pub box_shadow: Option<Vec<BoxShadow>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RescaleMode {
    /// Resize the component proportionally, so one of the dimensions is the same as its parent,
    /// but it still fits inside it.
    Fit,
    /// Resize the component proportionally, so one of the dimensions is the same as its parent and
    /// the entire area of the parent is covered. Parts of a child that do not fit inside the parent
    /// are not rendered.
    Fill,
}

/// WebView component renders a website using Chromium.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WebView {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// List of component's children.
    #[schema(no_recursion)]
    pub children: Option<Vec<Component>>,

    /// Id of a web renderer instance. It identifies an instance registered using the
    /// `POST /api/web-renderer/{instance_id}/register` request.
    ///
    /// :::warning
    /// You can only refer to specific instances in one Component at a time.
    /// :::
    pub instance_id: RendererId,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Image {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// Id of an image. It identifies an image registered using the
    /// `POST /api/image/{image_id}/register` request.
    pub image_id: RendererId,

    /// Width of the image in pixels. If `height` is not explicitly provided, the image will
    /// automatically adjust its height to maintain its original aspect ratio relative to the width.
    pub width: Option<f32>,

    /// Height of the image in pixels. If `width` is not explicitly provided, the image will
    /// automatically adjust its width to maintain its original aspect ratio relative to the height.
    pub height: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shader {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// List of component's children.
    #[schema(no_recursion)]
    pub children: Option<Vec<Component>>,

    /// Id of a shader. It identifies a shader registered using the
    /// `POST /api/shader/{shader_id}/register` request.
    pub shader_id: RendererId,
    /// Object that will be serialized into a `struct` and passed inside the shader as:
    ///
    /// ```wgsl
    /// @group(1) @binding(0) var<uniform>
    /// ```
    /// :::note
    ///   This object's structure must match the structure defined in a shader source code.
    ///   Currently, we do not handle memory layout automatically. To achieve the correct memory
    ///   alignment, you might need to pad your data with additional fields. See
    ///   [WGSL documentation](https://www.w3.org/TR/WGSL/#alignment-and-size) for more details.
    /// :::
    pub shader_param: Option<ShaderParam>,
    /// Resolution of a texture where shader will be executed.
    pub resolution: Resolution,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    content = "value",
    deny_unknown_fields
)]
pub enum ShaderParam {
    F32(f32),
    U32(u32),
    I32(i32),

    #[schema(no_recursion)]
    List(Vec<ShaderParam>),
    Struct(Vec<ShaderParamStructField>),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
pub struct ShaderParamStructField {
    pub field_name: String,
    #[serde(flatten)]
    #[schema(no_recursion)]
    pub value: ShaderParam,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Text {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// Text that will be rendered.
    pub text: Arc<str>,

    /// Width of a texture that text will be rendered on. If not provided, the resulting texture
    /// will be sized based on the defined text but limited to `max_width` value.
    pub width: Option<f32>,
    /// Height of a texture that text will be rendered on. If not provided, the resulting texture
    /// will be sized based on the defined text but limited to `max_height` value.
    /// It's an error to provide `height` if `width` is not defined.
    pub height: Option<f32>,
    /// Maximal `width`. Limits the width of the texture that the text will be rendered on. Value is
    /// ignored if `width` is defined.
    ///
    /// Defaults to `7682`.
    pub max_width: Option<f32>,
    /// Maximal `height`. Limits the height of the texture that the text will be rendered on. Value
    /// is ignored if height is defined.
    ///
    /// Defaults to `4320`.
    pub max_height: Option<f32>,

    /// Font size in pixels.
    pub font_size: f32,
    /// Distance between lines in pixels. Defaults to the value of the `font_size` property.
    pub line_height: Option<f32>,
    /// Font color in `#RRGGBBAA` format. Defaults to `"#FFFFFFFF"`.
    pub color: Option<RGBAColor>,
    /// Background color in `#RRGGBBAA` format. Defaults to `"#00000000"`.
    pub background_color: Option<RGBAColor>,
    /// Font family. Provide
    /// [family-name](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#family-name-value) for a
    /// specific font. "generic-family" values like e.g. "sans-serif" will not work.
    ///
    /// Defaults to `"Verdana"`.
    pub font_family: Option<Arc<str>>,
    /// Font style. The selected font needs to support the specified style. Defaults to `"normal"`.
    pub style: Option<TextStyle>,
    /// Text align. Defaults to `"left"`.
    pub align: Option<HorizontalAlign>,
    /// Text wrapping options. Defaults to `"none"`.
    pub wrap: Option<TextWrapMode>,
    /// Font weight. The selected font needs to support the specified weight.
    ///
    /// Defaults to `"normal"`.
    pub weight: Option<TextWeight>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TextWrapMode {
    /// Disable text wrapping. Text that does not fit inside the texture will be cut off.
    None,
    /// Wraps at a glyph level.
    Glyph,
    /// Wraps at a word level. Prevent splitting words when wrapping.
    Word,
}

/// Font weight, based on the
/// [OpenType specification](https://learn.microsoft.com/en-gb/typography/opentype/spec/os2#usweightclass).
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TextWeight {
    /// Weight 100.
    Thin,
    /// Weight 200.
    ExtraLight,
    /// Weight 300.
    Light,
    /// Weight 400.
    Normal,
    /// Weight 500.
    Medium,
    /// Weight 600.
    SemiBold,
    /// Weight 700.
    Bold,
    /// Weight 800.
    ExtraBold,
    /// Weight 900.
    Black,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    Spring,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tiles {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// List of component's children.
    #[schema(no_recursion)]
    pub children: Option<Vec<Component>>,

    /// Width of a component in pixels. Exact behavior might be different based on the parent
    /// component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub width: Option<f32>,
    /// Height of a component in pixels. Exact behavior might be different based on the parent
    /// component:
    /// - If the parent component is a layout, check sections "Absolute positioning" and "Static
    ///   positioning" of that component.
    /// - If the parent component is not a layout, then this field is required.
    pub height: Option<f32>,

    /// Background color in a `"#RRGGBBAA"` format. Defaults to `"#00000000"`.
    pub background_color: Option<RGBAColor>,
    /// Aspect ratio of a tile in `"W:H"` format, where W and H are integers. Defaults to `"16:9"`.
    pub tile_aspect_ratio: Option<AspectRatio>,
    /// Margin of each tile in pixels. Defaults to `0`.
    pub margin: Option<f32>,
    /// Padding on each tile in pixels. Defaults to `0`.
    pub padding: Option<f32>,
    /// Horizontal alignment of tiles. Defaults to `"center"`.
    pub horizontal_align: Option<HorizontalAlign>,
    /// Vertical alignment of tiles. Defaults to `"center"`.
    pub vertical_align: Option<VerticalAlign>,

    /// Defines how this component will behave during a scene update. This will only have an
    /// effect if the previous scene already contained a `Tiles` component with the same id.
    pub transition: Option<Transition>,
}
