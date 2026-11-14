use std::time::Duration;

use crate::state::{InputTextures, RenderCtx, node_texture::NodeTexture};
use crate::transformations::shader::validation::error::ParametersValidationError;
use crate::{OutputId, RendererId, Resolution};

use self::image_component::StatefulImageComponent;
use self::input_stream_component::StatefulInputStreamComponent;
use self::layout::{LayoutChild, LayoutRoot};
use self::scene_state::{BuildStateTreeCtx, LayoutCtx};
use self::shader_component::StatefulShaderComponent;
use self::text_component::StatefulTextComponent;
use self::web_view_component::StatefulWebViewComponent;

pub(crate) use scene_state::SceneState;

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
pub enum Scene {
    ComponentTree(Component),
    /// Root of the scene rendered for a different output, only the conversion to this output's
    /// frame format is done separately. If resolutions differ, the frame is resampled to this
    /// output's resolution without preserving the aspect ratio. Renders black frames while the
    /// other output does not render its own scene.
    LinkedOutput(OutputId),
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

/// Stateful version of a `Component` that is rendered to its own texture.
///
/// Represents the same element as `Component`, but additionally it has:
/// - state that is used to keep track of a transition or to preserve some information
///   from a previous scene update.
/// - everything that is necessary to render that component.
///
/// Layout components (`View`, `Rescaler`, `Tiles`) nested directly in each other are rendered
/// together to one texture. Only the outermost one is represented by
/// `StatefulComponent::Layout`, the rest of them are a `LayoutChild::Layout`.
enum StatefulComponent {
    InputStream(StatefulInputStreamComponent),
    Shader(StatefulShaderComponent),
    WebView(StatefulWebViewComponent),
    Image(StatefulImageComponent),
    Text(StatefulTextComponent),
    Layout(Box<LayoutRoot>),
}

impl StatefulComponent {
    /// Size of a texture that this component is rendered to.
    fn size(&self) -> Size {
        match self {
            StatefulComponent::InputStream(input) => input.size,
            StatefulComponent::Shader(shader) => shader.size,
            StatefulComponent::WebView(web) => web.size(),
            StatefulComponent::Image(image) => image.size(),
            StatefulComponent::Text(text) => text.size(),
            StatefulComponent::Layout(layout) => layout.size(),
        }
    }

    fn component_id(&self) -> Option<&ComponentId> {
        match self {
            StatefulComponent::InputStream(input) => input.component_id(),
            StatefulComponent::Shader(shader) => shader.component_id(),
            StatefulComponent::WebView(web) => web.component_id(),
            StatefulComponent::Image(image) => image.component_id(),
            StatefulComponent::Text(text) => text.component_id(),
            StatefulComponent::Layout(layout) => layout.component.component_id(),
        }
    }

    /// Calculates layout of this component and all its descendants for a single frame. It has
    /// to be called for every frame before `render`.
    fn layout(&mut self, ctx: &LayoutCtx) {
        match self {
            StatefulComponent::InputStream(input) => input.layout(ctx),
            StatefulComponent::Shader(shader) => {
                shader.children.iter_mut().for_each(|c| c.layout(ctx))
            }
            StatefulComponent::WebView(web) => web.children.iter_mut().for_each(|c| c.layout(ctx)),
            StatefulComponent::Image(_) | StatefulComponent::Text(_) => (),
            StatefulComponent::Layout(layout) => layout.layout(ctx),
        }
    }

    /// Renders this component and all its descendants.
    fn render(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        match self {
            // Textures of input streams are populated before the scene is rendered.
            StatefulComponent::InputStream(_) => (),
            StatefulComponent::Shader(shader) => shader.render(ctx, inputs, pts),
            StatefulComponent::WebView(web) => web.render(ctx, inputs, pts),
            StatefulComponent::Image(image) => image.render(ctx, pts),
            StatefulComponent::Text(text) => text.render(ctx),
            StatefulComponent::Layout(layout) => layout.render(ctx, inputs, pts),
        }
    }

    /// Texture with a result of the last `render` call.
    fn texture<'a>(&'a self, inputs: &'a InputTextures) -> &'a NodeTexture {
        match self {
            StatefulComponent::InputStream(input) => input.texture(inputs),
            StatefulComponent::Shader(shader) => &shader.output,
            StatefulComponent::WebView(web) => &web.output,
            StatefulComponent::Image(image) => &image.output,
            StatefulComponent::Text(text) => &text.output,
            StatefulComponent::Layout(layout) => &layout.output,
        }
    }
}

impl Component {
    /// Recursively convert `Component` tree provided by a user into a `StatefulComponent`
    /// tree. When calculating initial value of the state, the component has access to state
    /// of that component from before scene update.
    ///
    /// * `forced_size` - Forces size of a component, primary use case for this param is
    ///   to force size on a top level component to match resolution of an output stream.
    ///   TODO: Currently only layouts respect that value
    fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
        forced_size: Option<Size>,
    ) -> Result<StatefulComponent, SceneError> {
        let layout = match self {
            Component::InputStream(input) => return input.stateful_component(ctx),
            Component::Shader(shader) => return shader.stateful_component(ctx),
            Component::WebView(web_view) => return web_view.stateful_component(ctx),
            Component::Image(image) => return image.stateful_component(ctx),
            Component::Text(text) => return text.stateful_component(ctx),
            Component::View(view) => view.stateful_component(ctx)?,
            Component::Tiles(tiles) => tiles.stateful_component(ctx)?,
            Component::Rescaler(rescaler) => rescaler.stateful_component(ctx)?,
        };
        let root = LayoutRoot::new(layout, forced_size, ctx)?;
        Ok(StatefulComponent::Layout(Box::new(root)))
    }

    /// Same as `stateful_component`, but for a component that is a child of a layout component.
    fn layout_child(self, ctx: &BuildStateTreeCtx) -> Result<LayoutChild, SceneError> {
        let layout = match self {
            Component::View(view) => view.stateful_component(ctx)?,
            Component::Tiles(tiles) => tiles.stateful_component(ctx)?,
            Component::Rescaler(rescaler) => rescaler.stateful_component(ctx)?,
            component => {
                let node = component.stateful_component(ctx, None)?;
                return Ok(LayoutChild::Node(Box::new(node)));
            }
        };
        Ok(LayoutChild::Layout(Box::new(layout)))
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
