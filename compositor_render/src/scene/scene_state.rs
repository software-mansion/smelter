use std::{collections::HashMap, time::Duration};

use log::error;

use crate::{
    scene::image_component::ImageRenderParams, state::renderers::Renderers,
    transformations::text_renderer::TextRendererCtx, InputId, OutputId, Resolution,
};

use super::{
    image_component::StatefulImageComponent,
    input_stream_component::StatefulInputStreamComponent,
    layout::{LayoutNode, SizedLayoutComponent, StatefulLayoutComponent},
    shader_component::StatefulShaderComponent,
    text_component::StatefulTextComponent,
    validation::validate_scene_update,
    web_view_component::StatefulWebViewComponent,
    ComponentId, Node, NodeParams, OutputScene, Position, SceneError, Size, StatefulComponent,
};

pub(super) struct BuildStateTreeCtx<'a> {
    pub(super) prev_state: HashMap<ComponentId, &'a StatefulComponent>,
    pub(super) last_render_pts: Duration,
    pub(super) renderers: &'a Renderers,
    pub(super) text_renderer_ctx: &'a TextRendererCtx,
    pub(super) input_resolutions: &'a HashMap<InputId, Resolution>,
}

pub(crate) struct SceneState {
    output_scenes: HashMap<OutputId, OutputScene>,
    output_states: HashMap<OutputId, OutputSceneState>,
    last_pts: Duration,
    // Input resolutions from the last render
    input_resolutions: HashMap<InputId, Resolution>,
}

#[derive(Debug, Clone)]
struct OutputSceneState {
    root: StatefulComponent,
    resolution: Resolution,
}

pub(crate) struct OutputNode {
    pub(crate) output_id: OutputId,
    pub(crate) node: Node,
    pub(crate) resolution: Resolution,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            output_scenes: HashMap::new(),
            output_states: HashMap::new(),
            last_pts: Duration::ZERO,
            input_resolutions: HashMap::new(),
        }
    }

    pub(crate) fn register_render_event(
        &mut self,
        pts: Duration,
        input_resolutions: HashMap<InputId, Resolution>,
    ) {
        self.last_pts = pts;
        self.input_resolutions = input_resolutions;
        // TODO: pass input stream sizes and populate it in the ComponentState tree
    }

    pub(crate) fn unregister_output(&mut self, output_id: &OutputId) {
        self.output_scenes.remove(output_id);
        self.output_states.remove(output_id);
    }

    pub(crate) fn update_scene(
        &mut self,
        output_scene: OutputScene,
        renderers: &Renderers,
        text_renderer_ctx: &TextRendererCtx,
    ) -> Result<OutputNode, SceneError> {
        let output_id = output_scene.output_id.clone();
        validate_scene_update(&self.output_scenes, &output_scene)?;

        for (_, output) in self.output_states.iter_mut() {
            recalculate_layout(
                &mut output.root,
                Some(output.resolution.into()),
                self.last_pts,
                false,
            )
        }

        let ctx = BuildStateTreeCtx {
            prev_state: self
                .output_states
                .get(&output_id)
                .map(|o| {
                    let mut components = HashMap::new();
                    gather_components_with_id(&o.root, &mut components);
                    components
                })
                .unwrap_or_default(),
            last_render_pts: self.last_pts,
            input_resolutions: &self.input_resolutions,
            text_renderer_ctx,
            renderers,
        };

        let output_state_tree = OutputSceneState {
            root: output_scene.scene_root.clone().stateful_component(&ctx)?,
            resolution: output_scene.resolution,
        };

        let output_node_tree = OutputNode {
            output_id: output_id.clone(),
            node: output_state_tree
                .root
                .intermediate_node()
                .build_tree(Some(output_scene.resolution), self.last_pts)?,
            resolution: output_scene.resolution,
        };

        self.output_scenes.insert(output_id.clone(), output_scene);
        self.output_states.insert(output_id, output_state_tree);

        Ok(output_node_tree)
    }
}

/// Intermediate representation of a node tree while it's being constructed.
pub(super) enum IntermediateNode {
    InputStream(StatefulInputStreamComponent),
    Shader {
        shader: StatefulShaderComponent,
        children: Vec<IntermediateNode>,
    },
    WebView {
        web: StatefulWebViewComponent,
        children: Vec<IntermediateNode>,
    },
    Image(StatefulImageComponent),
    Text(StatefulTextComponent),
    Layout {
        root: StatefulLayoutComponent,
        children: Vec<IntermediateNode>,
    },
}

impl IntermediateNode {
    /// * `resolution` - Forces resolution of a node, primary use case for this
    ///   param is to force resolution on a top level component to match resolution
    ///   of an output stream. TODO: Currently only layouts respect that value
    /// * `pts` - PTS from the last render (this function is not called on render
    ///   so we can't have exact PTS here)
    fn build_tree(self, resolution: Option<Resolution>, pts: Duration) -> Result<Node, SceneError> {
        let size = match resolution {
            Some(resolution) => resolution.into(),
            None => self.node_size(pts)?,
        };
        match self {
            IntermediateNode::InputStream(input) => Ok(Node {
                params: NodeParams::InputStream(input.component.input_id), // TODO: enforce resolution
                children: vec![],
            }),
            IntermediateNode::Shader { shader, children } => Ok(Node {
                params: NodeParams::Shader(shader.component, shader.shader), // TODO: enforce resolution
                children: children
                    .into_iter()
                    .map(|node| node.build_tree(None, pts))
                    .collect::<Result<_, _>>()?,
            }),
            IntermediateNode::WebView { web, children } => Ok(Node {
                params: NodeParams::Web(web.children_ids, web.instance), // TODO: enforce resolution
                children: children
                    .into_iter()
                    .map(|node| node.build_tree(None, pts))
                    .collect::<Result<_, _>>()?,
            }),
            IntermediateNode::Layout { root, children } => Ok(Node {
                params: NodeParams::Layout(LayoutNode {
                    root: SizedLayoutComponent::new(root, size),
                }),
                children: children
                    .into_iter()
                    .map(|node| node.build_tree(None, pts))
                    .collect::<Result<_, _>>()?,
            }),
            IntermediateNode::Image(image) => Ok(Node {
                params: NodeParams::Image(ImageRenderParams {
                    image: image.image,
                    start_pts: image.start_pts,
                    resolution: image.resolution,
                }),
                children: vec![],
            }),
            IntermediateNode::Text(text) => Ok(Node {
                params: NodeParams::Text(text.params),
                children: vec![],
            }),
        }
    }

    fn node_size(&self, pts: Duration) -> Result<Size, SceneError> {
        match self {
            IntermediateNode::InputStream(input) => Ok(input.size),
            IntermediateNode::Shader {
                shader,
                children: _,
            } => Ok(shader.component.size),
            IntermediateNode::WebView { web, children: _ } => Ok(web.size()),
            IntermediateNode::Image(image) => Ok(image.size()),
            IntermediateNode::Text(text) => Ok(text.size()),
            IntermediateNode::Layout { root, children: _ } => {
                let (width, height) = match root.position(pts) {
                    Position::Static { width, height } => (width, height),
                    // Technically absolute positioning is a bug here, but I think throwing error
                    // in this case would be to invasive. It's better to just ignore those values.
                    Position::Absolute(position) => (position.width, position.height),
                };
                if let (Some(width), Some(height)) = (width, height) {
                    Ok(Size { width, height })
                } else {
                    Err(SceneError::UnknownDimensionsForLayoutNodeRoot {
                        component: root.component_type(),
                        msg: match root.component_id() {
                            Some(id) => format!("Please provide width and height values for component with id \"{id}\""),
                            None => "Please provide width and height values.".to_string(),
                        },
                    })
                }
            }
        }
    }
}

fn recalculate_layout(
    component: &mut StatefulComponent,
    size: Option<Size>,
    pts: Duration,
    parent_is_layout: bool,
) {
    let (width, height) = (component.width(pts), component.height(pts));
    match component {
        StatefulComponent::Layout(layout) => {
            if !parent_is_layout {
                let size = size.or_else(|| {
                    let (Some(width), Some(height)) = (width, height) else {
                        error!("Unknown dimensions on root layout component.");
                        return None;
                    };
                    Some(Size { width, height })
                });
                if let Some(size) = size {
                    layout.layout(size, pts);
                }
            }
            for child in layout.children_mut() {
                recalculate_layout(child, None, pts, true)
            }
        }
        component => {
            for child in component.children_mut() {
                recalculate_layout(child, None, pts, false)
            }
        }
    }
}

fn gather_components_with_id<'a>(
    component: &'a StatefulComponent,
    components: &mut HashMap<ComponentId, &'a StatefulComponent>,
) {
    match component {
        StatefulComponent::InputStream(input) => {
            if let Some(id) = input.component_id() {
                components.insert(id.clone(), component);
            }
        }
        StatefulComponent::Shader(shader) => {
            if let Some(id) = shader.component_id() {
                components.insert(id.clone(), component);
            }
            for child in shader.children.iter() {
                gather_components_with_id(child, components);
            }
        }
        StatefulComponent::WebView(web) => {
            if let Some(id) = web.component_id() {
                components.insert(id.clone(), component);
            }
            for child in web.children.iter() {
                gather_components_with_id(child, components);
            }
        }
        StatefulComponent::Image(image) => {
            if let Some(id) = image.component_id() {
                components.insert(id.clone(), component);
            }
        }
        StatefulComponent::Text(image) => {
            if let Some(id) = image.component_id() {
                components.insert(id.clone(), component);
            }
        }
        StatefulComponent::Layout(layout) => {
            if let Some(id) = layout.component_id() {
                components.insert(id.clone(), component);
            }
            for child in layout.children() {
                gather_components_with_id(child, components);
            }
        }
    }
}
