use std::{collections::HashMap, time::Duration};

use crate::{
    InputId, OutputId, Resolution,
    state::{InputTextures, RenderCtx, node_texture::NodeTexture},
};

use super::{
    ComponentId, OutputScene, SceneError, StatefulComponent,
    image_component::StatefulImageComponent,
    layout::{LayoutChild, StatefulLayoutComponent},
    validation::validate_scene_update,
};

pub(super) struct BuildStateTreeCtx<'a> {
    pub(super) prev_state: HashMap<ComponentId, PrevComponent<'a>>,
    pub(super) last_render_pts: Duration,
    pub(super) input_resolutions: &'a HashMap<InputId, Resolution>,
    pub(super) render_ctx: &'a RenderCtx<'a>,
}

/// State of a component from before the scene update.
#[derive(Clone, Copy)]
pub(super) enum PrevComponent<'a> {
    Layout(&'a StatefulLayoutComponent),
    Image(&'a StatefulImageComponent),
}

/// Values that can be different for every frame.
pub(super) struct LayoutCtx<'a> {
    pub(super) pts: Duration,
    pub(super) input_resolutions: &'a HashMap<InputId, Resolution>,
}

pub(crate) struct SceneState {
    output_scenes: HashMap<OutputId, OutputScene>,
    output_states: HashMap<OutputId, OutputSceneState>,
    last_pts: Duration,
}

/// State of a scene rendered for a single output.
pub(crate) struct OutputSceneState {
    root: StatefulComponent,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            output_scenes: HashMap::new(),
            output_states: HashMap::new(),
            last_pts: Duration::ZERO,
        }
    }

    pub(crate) fn clear_output_state(&mut self, output_id: &OutputId) {
        self.output_scenes.remove(output_id);
        self.output_states.remove(output_id);
    }

    /// Creates state for an updated scene. It is not used until it is passed
    /// to `apply_update`.
    ///
    /// * `input_resolutions` - Input resolutions from the last render.
    pub(crate) fn prepare_update(
        &self,
        output_scene: &OutputScene,
        render_ctx: &RenderCtx,
        input_resolutions: &HashMap<InputId, Resolution>,
    ) -> Result<OutputSceneState, SceneError> {
        validate_scene_update(&self.output_scenes, output_scene)?;

        let ctx = BuildStateTreeCtx {
            prev_state: self
                .output_states
                .get(&output_scene.output_id)
                .map(|o| {
                    let mut components = HashMap::new();
                    gather_components_with_id(&o.root, &mut components);
                    components
                })
                .unwrap_or_default(),
            last_render_pts: self.last_pts,
            input_resolutions,
            render_ctx,
        };

        let mut root = output_scene
            .scene_root
            .clone()
            .stateful_component(&ctx, Some(output_scene.resolution.into()))?;
        // The next scene update can happen before this scene is rendered, so the state that
        // depends on the layout has to be already there.
        root.layout(&LayoutCtx {
            pts: self.last_pts,
            input_resolutions,
        });

        Ok(OutputSceneState { root })
    }

    pub(crate) fn apply_update(&mut self, output_scene: OutputScene, state: OutputSceneState) {
        let output_id = output_scene.output_id.clone();
        self.output_scenes.insert(output_id.clone(), output_scene);
        self.output_states.insert(output_id, state);
    }

    /// Calculates layouts of all the scenes for a frame with specified PTS. It has to be
    /// called for every frame before `render`.
    pub(crate) fn layout(
        &mut self,
        pts: Duration,
        input_resolutions: &HashMap<InputId, Resolution>,
    ) {
        self.last_pts = pts;
        let ctx = LayoutCtx {
            pts,
            input_resolutions,
        };
        for output in self.output_states.values_mut() {
            output.root.layout(&ctx);
        }
    }

    pub(crate) fn render(&mut self, ctx: &mut RenderCtx, inputs: &InputTextures, pts: Duration) {
        for output in self.output_states.values_mut() {
            output.root.render(ctx, inputs, pts);
        }
    }

    /// Texture with the scene of a specified output.
    pub(crate) fn output_texture<'a>(
        &'a self,
        output_id: &OutputId,
        inputs: &'a InputTextures,
    ) -> Option<&'a NodeTexture> {
        self.output_states
            .get(output_id)
            .map(|output| output.root.texture(inputs))
    }
}

fn gather_components_with_id<'a>(
    component: &'a StatefulComponent,
    components: &mut HashMap<ComponentId, PrevComponent<'a>>,
) {
    match component {
        StatefulComponent::InputStream(_) | StatefulComponent::Text(_) => (),
        StatefulComponent::Shader(shader) => {
            for child in shader.children.iter() {
                gather_components_with_id(child, components);
            }
        }
        StatefulComponent::WebView(web) => {
            for child in web.children.iter() {
                gather_components_with_id(child, components);
            }
        }
        StatefulComponent::Image(image) => {
            if let Some(id) = image.component_id() {
                components.insert(id.clone(), PrevComponent::Image(image));
            }
        }
        StatefulComponent::Layout(layout) => {
            gather_layout_components_with_id(&layout.component, components)
        }
    }
}

fn gather_layout_components_with_id<'a>(
    layout: &'a StatefulLayoutComponent,
    components: &mut HashMap<ComponentId, PrevComponent<'a>>,
) {
    if let Some(id) = layout.component_id() {
        components.insert(id.clone(), PrevComponent::Layout(layout));
    }
    for child in layout.children() {
        match child {
            LayoutChild::Layout(layout) => gather_layout_components_with_id(layout, components),
            LayoutChild::Node(node) => gather_components_with_id(node, components),
        }
    }
}
