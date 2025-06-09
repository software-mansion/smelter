use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputStream {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// Id of an input. It identifies a stream registered using a [`RegisterInputStream`](../routes.md#register-input) request.
    pub input_id: InputId,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// List of component's children.
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

    /// Distance in pixels between this component's top edge and its parent's top edge (including a border).
    /// If this field is defined, then the component will ignore a layout defined by its parent.
    pub top: Option<f32>,
    /// Distance in pixels between this component's left edge and its parent's left edge (including a border).
    /// If this field is defined, this element will be absolutely positioned, instead of being
    /// laid out by its parent.
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

    /// (**default=`"hidden"`**) Controls what happens to content that is too big to fit into an area.
    pub overflow: Option<Overflow>,

    /// (**default=`"#00000000"`**) Background color in a `"#RRGGBBAA"` format.
    pub background_color: Option<RGBAColor>,

    /// (**default=`0.0`**) Radius of a rounded corner.
    pub border_radius: Option<f32>,

    /// (**default=`0.0`**) Border width.
    pub border_width: Option<f32>,

    /// (**default=`"#00000000"`**) Border color in a `"#RRGGBBAA"` format.
    pub border_color: Option<RGBAColor>,

    /// List of box shadows.
    pub box_shadow: Option<Vec<BoxShadow>>,

    /// (**default=`0.0`**) Padding for all sides of the component.
    pub padding: Option<f32>,

    /// (**default=`0.0`**) Padding for the top and bottom of the component.
    pub padding_vertical: Option<f32>,

    /// (**default=`0.0`**) Padding for the left and right of the component.
    pub padding_horizontal: Option<f32>,

    /// (**default=`0.0`**) Padding on top side in pixels.
    pub padding_top: Option<f32>,

    /// (**default=`0.0`**) Padding on right side in pixels.
    pub padding_right: Option<f32>,

    /// (**default=`0.0`**) Padding on bottom side in pixels.
    pub padding_bottom: Option<f32>,

    /// (**default=`0.0`**) Padding on left side in pixels.
    pub padding_left: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxShadow {
    pub offset_x: Option<f32>,
    pub offset_y: Option<f32>,
    pub color: Option<RGBAColor>,
    pub blur_radius: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    /// Render everything, including content that extends beyond their parent.
    Visible,
    /// Render only parts of the children that are inside their parent area.
    Hidden,
    /// If children components are too big to fit inside the parent, resize everything inside to fit.
    ///
    /// Components that have unknown sizes will be treated as if they had a size 0 when calculating
    /// scaling factor.
    ///
    /// :::warning
    /// This will resize everything inside, even absolutely positioned elements. For example, if
    /// you have an element in the bottom right corner and the content will be rescaled by a factor 0.5x,
    /// then that component will end up in the middle of its parent
    /// :::
    Fit,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewDirection {
    /// Children positioned from left to right.
    Row,
    /// Children positioned from top to bottom.
    Column,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rescaler {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// List of component's children.
    pub child: Box<Component>,

    /// (**default=`"fit"`**) Resize mode:
    pub mode: Option<RescaleMode>,
    /// (**default=`"center"`**) Horizontal alignment.
    pub horizontal_align: Option<HorizontalAlign>,
    /// (**default=`"center"`**) Vertical alignment.
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

    /// Distance in pixels between this component's top edge and its parent's top edge (including a border).
    /// If this field is defined, then the component will ignore a layout defined by its parent.
    pub top: Option<f32>,
    /// Distance in pixels between this component's left edge and its parent's left edge (including a border).
    /// If this field is defined, this element will be absolutely positioned, instead of being
    /// laid out by its parent.
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

    /// (**default=`0.0`**) Radius of a rounded corner.
    pub border_radius: Option<f32>,

    /// (**default=`0.0`**) Border width.
    pub border_width: Option<f32>,

    /// (**default=`"#00000000"`**) Border color in a `"#RRGGBBAA"` format.
    pub border_color: Option<RGBAColor>,

    /// List of box shadows.
    pub box_shadow: Option<Vec<BoxShadow>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RescaleMode {
    /// Resize the component proportionally, so one of the dimensions is the same as its parent,
    /// but it still fits inside it.
    Fit,
    /// Resize the component proportionally, so one of the dimensions is the same as its parent
    /// and the entire area of the parent is covered. Parts of a child that do not fit inside the parent are not rendered.
    Fill,
}

/// WebView component renders a website using Chromium.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebView {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// List of component's children.
    pub children: Option<Vec<Component>>,

    /// Id of a web renderer instance. It identifies an instance registered using a
    /// [`register web renderer`](../routes.md#register-web-renderer-instance) request.
    ///
    /// :::warning
    /// You can only refer to specific instances in one Component at a time.
    /// :::
    pub instance_id: RendererId,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Image {
    /// Id of a component.
    pub id: Option<ComponentId>,

    /// Id of an image. It identifies an image registered using a [`register image`](../routes.md#register-image) request.
    pub image_id: RendererId,

    pub width: Option<usize>,

    pub height: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shader {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// List of component's children.
    pub children: Option<Vec<Component>>,

    /// Id of a shader. It identifies a shader registered using a [`register shader`](../routes.md#register-shader) request.
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
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
    List(Vec<ShaderParam>),
    Struct(Vec<ShaderParamStructField>),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ShaderParamStructField {
    pub field_name: String,
    #[serde(flatten)]
    pub value: ShaderParam,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
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
    /// (**default=`7682`**) Maximal `width`. Limits the width of the texture that the text will be rendered on.
    /// Value is ignored if `width` is defined.
    pub max_width: Option<f32>,
    /// (**default=`4320`**) Maximal `height`. Limits the height of the texture that the text will be rendered on.
    /// Value is ignored if height is defined.
    pub max_height: Option<f32>,

    /// Font size in pixels.
    pub font_size: f32,
    /// Distance between lines in pixels. Defaults to the value of the `font_size` property.
    pub line_height: Option<f32>,
    /// (**default=`"#FFFFFFFF"`**) Font color in `#RRGGBBAA` format.
    pub color: Option<RGBAColor>,
    /// (**default=`"#00000000"`**) Background color in `#RRGGBBAA` format.
    pub background_color: Option<RGBAColor>,
    /// (**default=`"Verdana"`**) Font family. Provide [family-name](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#family-name-value)
    /// for a specific font. "generic-family" values like e.g. "sans-serif" will not work.
    pub font_family: Option<Arc<str>>,
    /// (**default=`"normal"`**) Font style. The selected font needs to support the specified style.
    pub style: Option<TextStyle>,
    /// (**default=`"left"`**) Text align.
    pub align: Option<HorizontalAlign>,
    /// (**default=`"none"`**) Text wrapping options.
    pub wrap: Option<TextWrapMode>,
    /// (**default=`"normal"`**) Font weight. The selected font needs to support the specified weight.
    pub weight: Option<TextWeight>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextWrapMode {
    /// Disable text wrapping. Text that does not fit inside the texture will be cut off.
    None,
    /// Wraps at a glyph level.
    Glyph,
    /// Wraps at a word level. Prevent splitting words when wrapping.
    Word,
}

/// Font weight, based on the [OpenType specification](https://learn.microsoft.com/en-gb/typography/opentype/spec/os2#usweightclass).
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    Spring,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tiles {
    /// Id of a component.
    pub id: Option<ComponentId>,
    /// List of component's children.
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

    /// (**default=`"#00000000"`**) Background color in a `"#RRGGBBAA"` format.
    pub background_color: Option<RGBAColor>,
    /// (**default=`"16:9"`**) Aspect ratio of a tile in `"W:H"` format, where W and H are integers.
    pub tile_aspect_ratio: Option<AspectRatio>,
    /// (**default=`0`**) Margin of each tile in pixels.
    pub margin: Option<f32>,
    /// (**default=`0`**) Padding on each tile in pixels.
    pub padding: Option<f32>,
    /// (**default=`"center"`**) Horizontal alignment of tiles.
    pub horizontal_align: Option<HorizontalAlign>,
    /// (**default=`"center"`**) Vertical alignment of tiles.
    pub vertical_align: Option<VerticalAlign>,

    /// Defines how this component will behave during a scene update. This will only have an
    /// effect if the previous scene already contained a `Tiles` component with the same id.
    pub transition: Option<Transition>,
}
