use compositor_render::{
    scene::{
        Component, HorizontalAlign, InputStreamComponent, RGBAColor, TilesComponent, VerticalAlign,
    },
    InputId, OutputId,
};

pub struct SceneContext {
    pub inputs: Vec<InputId>,
    #[allow(dead_code)]
    pub outputs: Vec<OutputId>,
}

pub fn simple_tiles_with_all_inputs(ctx: &SceneContext, _output_id: &OutputId) -> Component {
    Component::Tiles(TilesComponent {
        id: None,
        width: None,
        height: None,
        margin: 2.0,
        padding: 0.0,
        children: ctx
            .inputs
            .iter()
            .map(|input_id| {
                Component::InputStream(InputStreamComponent {
                    id: None,
                    input_id: input_id.clone(),
                })
            })
            .collect(),
        transition: None,
        vertical_align: VerticalAlign::Center,
        horizontal_align: HorizontalAlign::Center,
        background_color: RGBAColor(128, 128, 128, 0),
        tile_aspect_ratio: (16, 9),
    })
}
