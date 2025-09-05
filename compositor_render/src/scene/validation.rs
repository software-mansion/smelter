use std::collections::{HashMap, HashSet};

use crate::{OutputId, RendererId};

use super::{Component, ComponentId, OutputScene, SceneError};

impl Component {
    fn component_id(&self) -> Option<&ComponentId> {
        match self {
            Component::InputStream(input) => input.id.as_ref(),
            Component::Shader(shader) => shader.id.as_ref(),
            Component::WebView(web) => web.id.as_ref(),
            Component::Image(image) => image.id.as_ref(),
            Component::Text(text) => text.id.as_ref(),
            Component::View(view) => view.id.as_ref(),
            Component::Tiles(tiles) => tiles.id.as_ref(),
            Component::Rescaler(rescaler) => rescaler.id.as_ref(),
        }
    }

    fn children(&self) -> Vec<&Component> {
        match self {
            Component::InputStream(_input) => vec![],
            Component::Shader(shader) => shader.children.iter().collect(),
            Component::WebView(view) => view.children.iter().collect(),
            Component::Image(_image) => vec![],
            Component::Text(_text) => vec![],
            Component::View(view) => view.children.iter().collect(),
            Component::Tiles(tiles) => tiles.children.iter().collect(),
            Component::Rescaler(rescaler) => vec![rescaler.child.as_ref()],
        }
    }
}

pub(super) fn validate_scene_update(
    old_outputs: &HashMap<OutputId, OutputScene>,
    updated_output: &OutputScene,
) -> Result<(), SceneError> {
    let mut updated_outputs: HashMap<&OutputId, &OutputScene> = old_outputs.iter().collect();
    updated_outputs.insert(&updated_output.output_id, updated_output);
    let updated_outputs: Vec<&OutputScene> = updated_outputs.values().copied().collect();

    validate_component_ids_uniqueness(&updated_outputs)?;
    validate_web_renderer_ids_uniqueness(&updated_outputs)?;
    Ok(())
}

fn validate_component_ids_uniqueness(outputs: &[&OutputScene]) -> Result<(), SceneError> {
    fn visit<'a>(
        component: &'a Component,
        ids: &mut HashSet<&'a ComponentId>,
    ) -> Result<(), SceneError> {
        let id = component.component_id();
        if let Some(id) = id {
            if ids.contains(id) {
                return Err(SceneError::DuplicateComponentId(id.clone()));
            }

            ids.insert(id);
        }

        component
            .children()
            .into_iter()
            .try_for_each(|c| visit(c, ids))
    }

    outputs.iter().try_for_each(|output| {
        let mut ids: HashSet<&ComponentId> = HashSet::new();
        visit(&output.scene_root, &mut ids)
    })
}

fn validate_web_renderer_ids_uniqueness(outputs: &[&OutputScene]) -> Result<(), SceneError> {
    let mut web_renderer_ids: HashSet<&RendererId> = HashSet::new();

    fn visit<'a>(
        component: &'a Component,
        ids: &mut HashSet<&'a RendererId>,
    ) -> Result<(), SceneError> {
        if let Component::WebView(web_view) = component {
            let instance_id = &web_view.instance_id;
            if ids.contains(instance_id) {
                return Err(SceneError::WebRendererUsageNotExclusive(
                    instance_id.clone(),
                ));
            }
            ids.insert(instance_id);
        }

        component
            .children()
            .into_iter()
            .try_for_each(|c| visit(c, ids))
    }

    outputs
        .iter()
        .try_for_each(|output| visit(&output.scene_root, &mut web_renderer_ids))
}
