use crate::Resolution;

use super::{
    ComponentId, InputStreamComponent, SceneError, Size, StatefulComponent,
    scene_state::BuildStateTreeCtx,
};

#[derive(Debug, Clone)]
pub(super) struct StatefulInputStreamComponent {
    pub(super) component: InputStreamComponent,
    pub(super) size: Size,
}

impl StatefulInputStreamComponent {
    pub(super) fn component_id(&self) -> Option<&ComponentId> {
        self.component.id.as_ref()
    }
}

impl InputStreamComponent {
    pub(super) fn stateful_component(
        self,
        ctx: &BuildStateTreeCtx,
    ) -> Result<StatefulComponent, SceneError> {
        let input = ctx
            .input_resolutions
            .get(&self.input_id)
            .copied()
            .unwrap_or(Resolution {
                width: 0,
                height: 0,
            });
        Ok(StatefulComponent::InputStream(
            StatefulInputStreamComponent {
                component: self,
                size: input.into(),
            },
        ))
    }
}
