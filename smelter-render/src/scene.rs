use std::sync::Arc;
use std::time::Duration;

use crate::scene::image_component::ImageRenderParams;
use crate::transformations::shader::Shader;
use crate::transformations::shader::validation::error::ParametersValidationError;
use crate::transformations::text_renderer::TextRenderParams;
use crate::transformations::web_renderer::WebRenderer;
use crate::{InputId, OutputId, RendererId, Resolution};

use self::image_component::StatefulImageComponent;
use self::input_stream_component::StatefulInputStreamComponent;
use self::layout::{SizedLayoutComponent, StatefulLayoutComponent};
use self::scene_state::BuildStateTreeCtx;
use self::shader_component::StatefulShaderComponent;
use self::text_component::StatefulTextComponent;
use self::web_view_component::StatefulWebViewComponent;

pub(crate) use layout::LayoutNode;
pub(crate) use scene_state::SceneState;
pub(crate) use shader_component::ShaderComponentParams;

pub use components::*;
pub use types::*;

mod components;
pub(super) mod image_component;
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

#[derive(Debug, Clone, PartialEq)]
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
    Layout(Box<StatefulLayoutComponent>),
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

    /// Children of a `RenderNode` created from this component. Nested layout components are
    /// rendered by a single `RenderNode`, so their non-layout descendants are flattened.
    fn node_children(&self) -> Vec<&StatefulComponent> {
        match self {
            StatefulComponent::Shader(shader) => shader.children.iter().collect(),
            StatefulComponent::WebView(web) => web.children.iter().collect(),
            StatefulComponent::Layout(layout) => layout.node_children(),
            StatefulComponent::InputStream(_)
            | StatefulComponent::Image(_)
            | StatefulComponent::Text(_) => vec![],
        }
    }

    /// * `resolution` - Forces resolution of a node, primary use case for this param is to force
    ///   resolution on a top level component to match resolution of an output stream.
    ///   TODO: Currently only layouts respect that value
    fn node_params(
        &self,
        resolution: Option<Resolution>,
        pts: Duration,
    ) -> Result<NodeParams, SceneError> {
        let params = match self {
            StatefulComponent::InputStream(input) => {
                NodeParams::InputStream(input.component.input_id.clone())
            }
            StatefulComponent::Shader(shader) => {
                NodeParams::Shader(shader.component.clone(), shader.shader.clone())
            }
            StatefulComponent::WebView(web) => {
                NodeParams::Web(web.children_ids.clone(), web.instance.clone())
            }
            StatefulComponent::Image(image) => NodeParams::Image(image.image_render_params()),
            StatefulComponent::Text(text) => NodeParams::Text(text.params.clone()),
            StatefulComponent::Layout(layout) => {
                let size = match resolution {
                    Some(resolution) => resolution.into(),
                    None => layout.root_size(pts)?,
                };
                NodeParams::Layout(LayoutNode {
                    root: SizedLayoutComponent::new(layout.clone(), size),
                })
            }
        };
        Ok(params)
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
    #[error(
        "\"{component}\" that is a child of an non-layout component e.g. \"Shader\", \"WebView\" need to have known size. {msg}"
    )]
    UnknownDimensionsForLayoutNodeRoot {
        component: &'static str,
        msg: String,
    },

    #[error(
        "Image \"{0}\" does not exist. You have to register it first before using it in the scene definition."
    )]
    ImageNotFound(RendererId),

    #[error(
        "Shader \"{0}\" does not exist. You have to register it first before using it in the scene definition."
    )]
    ShaderNotFound(RendererId),

    #[error(
        "Instance of web renderer \"{0}\" does not exist. You have to register it first before using it in the scene definition."
    )]
    WebRendererNotFound(RendererId),

    #[error(
        "Instance of web renderer \"{0}\" was used more than once. Only one component can use specific web renderer at the time."
    )]
    WebRendererUsageNotExclusive(RendererId),

    #[error("WebView using \"{0}\" web renderer has children without \"id\" property")]
    WebViewChildWithoutId(RendererId),

    #[error("Invalid parameter passed to \"{1}\" shader.")]
    ShaderNodeParametersValidationError(#[source] ParametersValidationError, RendererId),

    #[error(
        "More than one component has an id \"{0}\". Component IDs in scene definition need to be unique."
    )]
    DuplicateComponentId(ComponentId),
}
