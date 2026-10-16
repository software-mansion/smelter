use std::{collections::HashMap, time::Duration};

use tracing::error;

use crate::{
    InputId, OutputId, Resolution,
    state::{RenderCtx, node::RenderNode, renderers::Renderers},
    transformations::text_renderer::TextRendererCtx,
};

use super::{
    ComponentId, OutputScene, SceneError, Size, StatefulComponent,
    validation::validate_scene_update,
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
    ) -> Result<(), SceneError> {
        let output_id = output_scene.output_id.clone();
        validate_scene_update(&self.output_scenes, &output_scene)?;

        for output in self.output_states.values_mut() {
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
        validate_node_sizes(&output_state_tree.root, self.last_pts)?;

        self.output_scenes.insert(output_id.clone(), output_scene);
        self.output_states.insert(output_id, output_state_tree);

        Ok(())
    }

    /// Walks the scene state of an output together with its render tree and creates render
    /// nodes that are missing.
    pub(crate) fn sync_render_tree(
        &self,
        ctx: &RenderCtx,
        output_id: &OutputId,
        root: &mut Option<RenderNode>,
    ) {
        let Some(output) = self.output_states.get(output_id) else {
            return;
        };
        match root {
            Some(node) => sync_render_node(node, &output.root),
            None => {
                match create_render_node(ctx, &output.root, Some(output.resolution), self.last_pts)
                {
                    Ok(node) => *root = Some(node),
                    Err(err) => {
                        error!("Failed to create render tree for output {output_id}: {err}")
                    }
                }
            }
        }
    }
}

fn create_render_node(
    ctx: &RenderCtx,
    component: &StatefulComponent,
    resolution: Option<Resolution>,
    pts: Duration,
) -> Result<RenderNode, SceneError> {
    let children = component
        .node_children()
        .into_iter()
        .map(|child| create_render_node(ctx, child, None, pts))
        .collect::<Result<_, _>>()?;
    Ok(RenderNode::new(
        ctx,
        component.node_params(resolution, pts)?,
        children,
    ))
}

fn sync_render_node(node: &mut RenderNode, component: &StatefulComponent) {
    for (child_node, child_component) in node.children.iter_mut().zip(component.node_children()) {
        sync_render_node(child_node, child_component);
    }
}

/// Layouts that are not a root of an output need to have known size.
fn validate_node_sizes(component: &StatefulComponent, pts: Duration) -> Result<(), SceneError> {
    for child in component.node_children() {
        if let StatefulComponent::Layout(layout) = child {
            layout.root_size(pts)?;
        }
        validate_node_sizes(child, pts)?;
    }
    Ok(())
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
