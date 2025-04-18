use std::sync::Arc;

use compositor_render::{
    scene::{
        Component, ImageComponent, InputStreamComponent, RGBAColor, RescalerComponent,
        TilesComponent, ViewComponent,
    },
    InputId, OutputId, RendererId,
};

pub struct SceneContext {
    pub inputs: Vec<InputId>,
    #[allow(dead_code)]
    pub outputs: Vec<OutputId>,
}

pub type SceneBuilderFn = fn(ctx: &SceneContext, output_id: &OutputId) -> Component;

pub fn simple_tiles_with_all_inputs(ctx: &SceneContext, _output_id: &OutputId) -> Component {
    Component::Tiles(TilesComponent {
        margin: 2.0,
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
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn blank(_ctx: &SceneContext, _output_id: &OutputId) -> Component {
    Component::View(ViewComponent {
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn static_image(_ctx: &SceneContext, _output_id: &OutputId) -> Component {
    Component::Rescaler(RescalerComponent {
        child: Component::Image(ImageComponent {
            id: None,
            image_id: RendererId(Arc::from("example_image")),
        })
        .into(),
        ..Default::default()
    })
}

pub fn single_video_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, id)| id == &output_id)
        .unwrap();

    if ctx.inputs.len() == 0 {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::Tiles(TilesComponent {
        margin: 2.0,
        children: vec![Component::InputStream(InputStreamComponent { id: None, input_id }).into()],
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn two_video_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, id)| id == &output_id)
        .unwrap();

    if ctx.inputs.len() == 0 {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 2) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 2 + 1) % ctx.inputs.len()].clone();

    Component::Tiles(TilesComponent {
        margin: 2.0,
        children: vec![
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_1,
            })
            .into(),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_2,
            })
            .into(),
        ],
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn four_video_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, id)| id == &output_id)
        .unwrap();

    if ctx.inputs.len() == 0 {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 4) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 4 + 1) % ctx.inputs.len()].clone();
    let input_3 = ctx.inputs[(output_index * 4 + 2) % ctx.inputs.len()].clone();
    let input_4 = ctx.inputs[(output_index * 4 + 3) % ctx.inputs.len()].clone();

    Component::Tiles(TilesComponent {
        margin: 2.0,
        children: vec![
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_1,
            })
            .into(),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_2,
            })
            .into(),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_3,
            })
            .into(),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_4,
            })
            .into(),
        ],
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

// One less copy than single_video_layout
pub fn single_video_pass_through(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, id)| id == &output_id)
        .unwrap();

    if ctx.inputs.len() == 0 {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::InputStream(InputStreamComponent { id: None, input_id }).into()
}
