use std::sync::Arc;
use std::time::Duration;

use crate::scene::image_component::ImageRenderParams;
use crate::transformations::image::Image;
use crate::transformations::shader::validation::error::ParametersValidationError;
use crate::transformations::shader::Shader;
use crate::transformations::text_renderer::TextRenderParams;
use crate::transformations::web_renderer::WebRenderer;
use crate::{InputId, OutputId, RendererId, Resolution};

use self::image_component::StatefulImageComponent;
use self::input_stream_component::StatefulInputStreamComponent;
use self::layout::StatefulLayoutComponent;
use self::scene_state::{BuildStateTreeCtx, IntermediateNode};
use self::shader_component::StatefulShaderComponent;
use self::text_component::StatefulTextComponent;
use self::web_view_component::StatefulWebViewComponent;

pub(crate) use layout::LayoutNode;
pub(crate) use scene_state::{OutputNode, SceneState};
pub(crate) use shader_component::ShaderComponentParams;

pub use components::*;
pub use types::*;

mod components;
mod image_component;
mod input_stream_component;
mod layout;
mod rescaler_component;
mod scene_state;
mod shader_component;
mod text_component;
mod tiles_component;
mod transition;
mod types;
mod validation;
mod view_component;
mod web_view_component;

#[derive(Debug, Clone)]
pub struct OutputScene {
    pub output_id: OutputId,
    pub scene_root: Component,
    pub resolution: Resolution,
}

#[derive(Debug, Clone)]
pub enum Component {
    InputStream(InputStreamComponent),
    Shader(ShaderComponent),
    WebView(WebViewComponent),
    Image(ImageComponent),
    Text(TextComponent),
    View(ViewComponent),
    Tiles(TilesComponent),
    Rescaler(RescalerComponent),
}

/// Stateful version of a `Component`. Represents the same element as
/// `Component`, but additionally it has its own state that can be used
/// keep track of transition or to preserve some information from
/// a previous scene update.
#[derive(Debug, Clone)]
enum StatefulComponent {
    InputStream(StatefulInputStreamComponent),
    Shader(StatefulShaderComponent),
    WebView(StatefulWebViewComponent),
    Image(StatefulImageComponent),
    Text(StatefulTextComponent),
    Layout(StatefulLayoutComponent),
}

/// Defines a tree structure that is a base to construct a `RenderGraph`.
/// Each `prams` element defines a parameters to construct a `RenderNode`
/// and `children` define connections between them.
///
/// In most cases each `Node` will be used to construct a RenderNode, but
/// in some cases multiple Nodes might be reduced to just one RenderNode
/// e.g. `NodeParams::InputStream` for the same input stream might be present
/// multiple times inside the tree, but it will result in only one `RenderNode`
/// in the `RenderGraph`
#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) params: NodeParams,
    pub(crate) children: Vec<Node>,
}

/// Set of params used to construct a `RenderNode`.
#[derive(Debug)]
pub(crate) enum NodeParams {
    InputStream(InputId),
    Shader(ShaderComponentParams, Arc<Shader>),
    Web(Vec<ComponentId>, Arc<WebRenderer>),
    Image(ImageRenderParams),
    Text(TextRenderParams),
    Layout(LayoutNode),
}

impl StatefulComponent {
    fn width(&self, pts: Duration) -> Option<f32> {
        match self {
            StatefulComponent::InputStream(input) => Some(input.size.width),
            StatefulComponent::Shader(shader) => Some(shader.component.size.width),
            StatefulComponent::WebView(web) => Some(web.size().width),
            StatefulComponent::Image(image) => Some(image.width()),
            StatefulComponent::Text(text) => Some(text.width()),
            StatefulComponent::Layout(layout) => match layout.position(pts) {
                Position::Static { width, .. } => width,
                Position::Absolute(position) => position.width,
            },
        }
    }

    fn height(&self, pts: Duration) -> Option<f32> {
        match self {
            StatefulComponent::InputStream(input) => Some(input.size.height),
            StatefulComponent::Shader(shader) => Some(shader.component.size.height),
            StatefulComponent::WebView(web) => Some(web.size().height),
            StatefulComponent::Image(image) => Some(image.height()),
            StatefulComponent::Text(text) => Some(text.height()),
            StatefulComponent::Layout(layout) => match layout.position(pts) {
                Position::Static { height, .. } => height,
                Position::Absolute(position) => position.height,
            },
        }
    }

    fn intermediate_node(&self) -> IntermediateNode {
        match self {
            StatefulComponent::InputStream(input) => input.intermediate_node(),
            StatefulComponent::Shader(shader) => shader.intermediate_node(),
            StatefulComponent::WebView(web) => web.intermediate_node(),
            StatefulComponent::Image(image) => image.intermediate_node(),
            StatefulComponent::Text(text) => text.intermediate_node(),
            StatefulComponent::Layout(layout) => match layout {
                StatefulLayoutComponent::View(view) => view.intermediate_node(),
                StatefulLayoutComponent::Tiles(tiles) => tiles.intermediate_node(),
                StatefulLayoutComponent::Rescaler(rescaler) => rescaler.intermediate_node(),
            },
        }
    }

    fn children_mut(&mut self) -> Vec<&mut StatefulComponent> {
        match self {
            StatefulComponent::InputStream(_) => vec![],
            StatefulComponent::Shader(shader) => shader.children.iter_mut().collect(),
            StatefulComponent::WebView(web) => web.children.iter_mut().collect(),
            StatefulComponent::Image(_) => vec![],
            StatefulComponent::Text(_) => vec![],
            StatefulComponent::Layout(layout) => layout.children_mut(),
        }
    }

    fn component_id(&self) -> Option<&ComponentId> {
        match self {
            StatefulComponent::InputStream(input) => input.component_id(),
            StatefulComponent::Shader(shader) => shader.component_id(),
            StatefulComponent::WebView(web) => web.component_id(),
            StatefulComponent::Image(image) => image.component_id(),
            StatefulComponent::Text(text) => text.component_id(),
            StatefulComponent::Layout(layout) => layout.component_id(),
        }
    }
}

impl Component {
    /// Recursively convert `Component` tree provided by a user into a
    /// `ComponentState` tree. `ComponentState` includes all the information
    /// from `Component`, but additionally it has it's own state. When calculating
    /// initial value of that state, the component has access to state of that
    /// component from before scene update.
    fn stateful_component(self, ctx: &BuildStateTreeCtx) -> Result<StatefulComponent, SceneError> {
        match self {
            Component::InputStream(input) => input.stateful_component(ctx),
            Component::Shader(shader) => shader.stateful_component(ctx),
            Component::WebView(web_view) => web_view.stateful_component(ctx),
            Component::Image(image) => image.stateful_component(ctx),
            Component::Text(text) => text.stateful_component(ctx),
            Component::View(view) => view.stateful_component(ctx),
            Component::Tiles(tiles) => tiles.stateful_component(ctx),
            Component::Rescaler(rescaler) => rescaler.stateful_component(ctx),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("\"{component}\" that is a child of an non-layout component e.g. \"Shader\", \"WebView\" need to have known size. {msg}")]
    UnknownDimensionsForLayoutNodeRoot {
        component: &'static str,
        msg: String,
    },

    #[error("Image \"{0}\" does not exist. You have to register it first before using it in the scene definition.")]
    ImageNotFound(RendererId),

    #[error("Shader \"{0}\" does not exist. You have to register it first before using it in the scene definition.")]
    ShaderNotFound(RendererId),

    #[error("Instance of web renderer \"{0}\" does not exist. You have to register it first before using it in the scene definition.")]
    WebRendererNotFound(RendererId),

    #[error(
        "Instance of web renderer \"{0}\" was used more than once. Only one component can use specific web renderer at the time."
    )]
    WebRendererUsageNotExclusive(RendererId),

    #[error("WebView using \"{0}\" web renderer has children without \"id\" property")]
    WebViewChildWithoutId(RendererId),

    #[error("Invalid parameter passed to \"{1}\" shader.")]
    ShaderNodeParametersValidationError(#[source] ParametersValidationError, RendererId),

    #[error("More than one component has an id \"{0}\". Component IDs in scene definition need to be unique.")]
    DuplicateComponentId(ComponentId),
}
