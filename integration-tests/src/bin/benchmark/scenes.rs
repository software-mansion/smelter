use std::sync::Arc;

use smelter_render::{
    InputId, OutputId, RendererId, RendererSpec,
    image::{ImageSource, ImageSpec, ImageType},
    scene::{
        Component, ImageComponent, InputStreamComponent, RGBAColor, RescalerComponent,
        ShaderComponent, Size, TilesComponent, ViewComponent,
    },
    shader::ShaderSpec,
};

use crate::utils::example_image_path;

pub struct SceneContext {
    pub inputs: Vec<InputId>,
    #[allow(dead_code)]
    pub outputs: Vec<OutputId>,
}

pub type SceneBuilderFn = fn(ctx: &SceneContext, output_id: &OutputId) -> Component;

pub fn example_image() -> (RendererId, RendererSpec) {
    (
        RendererId(Arc::from("example_image")),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: example_image_path().to_string_lossy().to_string(),
            },
            image_type: ImageType::Png,
        }),
    )
}

pub fn example_shader() -> (RendererId, RendererSpec) {
    (
        RendererId(Arc::from("example_shader")),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./silly.wgsl").into(),
        }),
    )
}

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
    let renderer_id = RendererId(Arc::from("example_image"));
    Component::Rescaler(RescalerComponent {
        child: Component::Image(ImageComponent {
            id: None,
            image_id: renderer_id.clone(),
            width: None,
            height: None,
        })
        .into(),
        ..Default::default()
    })
}

pub fn image_with_shader(_ctx: &SceneContext, _output_id: &OutputId) -> Component {
    Component::Shader(ShaderComponent {
        children: vec![Component::Image(ImageComponent {
            id: None,
            image_id: example_image().0,
            width: None,
            height: None,
        })],
        id: None,
        shader_id: example_shader().0,
        shader_param: None,
        size: Size {
            width: 1920.0,
            height: 1080.0,
        },
    })
}

pub fn single_video_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, id)| id == &output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::Tiles(TilesComponent {
        margin: 2.0,
        children: vec![Component::InputStream(InputStreamComponent {
            id: None,
            input_id,
        })],
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

    if ctx.inputs.is_empty() {
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
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_2,
            }),
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

    if ctx.inputs.is_empty() {
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
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_2,
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_3,
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_4,
            }),
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

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::InputStream(InputStreamComponent { id: None, input_id })
}
