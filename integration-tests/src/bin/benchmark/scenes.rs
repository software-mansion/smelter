use std::sync::Arc;

use smelter_render::{
    InputId, OutputId, RendererId, RendererSpec,
    image::{ImageSource, ImageSpec, ImageType},
    scene::{
        AbsolutePosition, Component, HorizontalPosition, ImageComponent, InputStreamComponent,
        Position, RGBAColor, RescaleMode, RescalerComponent, ShaderComponent, Size, TilesComponent,
        VerticalPosition, ViewChildrenDirection, ViewComponent,
    },
    shader::ShaderSpec,
};

use crate::{args::Resolution, utils::example_image_path};

pub struct SceneContext {
    pub inputs: Vec<InputId>,
    pub outputs: Vec<(OutputId, Resolution)>,
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
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::Rescaler(RescalerComponent {
        child: Box::new(Component::InputStream(InputStreamComponent {
            id: None,
            input_id,
        })),
        ..Default::default()
    })
}

pub fn two_video_side_by_fit_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 2) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 2 + 1) % ctx.inputs.len()].clone();

    // It will have a lot of blank space that effects encoding performance
    Component::View(ViewComponent {
        direction: ViewChildrenDirection::Row,
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

pub fn two_video_side_by_side_fill_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 2) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 2 + 1) % ctx.inputs.len()].clone();

    // fill to avoid blank space that affects encoding performance
    Component::View(ViewComponent {
        direction: ViewChildrenDirection::Row,
        children: vec![
            Component::Rescaler(RescalerComponent {
                child: Box::new(Component::InputStream(InputStreamComponent {
                    id: None,
                    input_id: input_1,
                })),
                mode: RescaleMode::Fill,
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                child: Box::new(Component::InputStream(InputStreamComponent {
                    id: None,
                    input_id: input_2,
                })),
                mode: RescaleMode::Fill,
                ..Default::default()
            }),
        ],
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn two_video_picture_in_picture_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, (_, output_resolution)) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 2) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 2 + 1) % ctx.inputs.len()].clone();

    let background = Component::InputStream(InputStreamComponent {
        id: None,
        input_id: input_1,
    });
    let pip = Component::Rescaler(RescalerComponent {
        child: Box::new(Component::InputStream(InputStreamComponent {
            id: None,
            input_id: input_2,
        })),
        position: Position::Absolute(AbsolutePosition {
            width: Some(output_resolution.width as f32 / 4.0),
            height: Some(output_resolution.height as f32 / 4.0),
            position_horizontal: HorizontalPosition::RightOffset(0.0),
            position_vertical: VerticalPosition::TopOffset(0.0),
            rotation_degrees: 0.0,
        }),
        mode: RescaleMode::Fill,
        ..Default::default()
    });

    Component::View(ViewComponent {
        direction: ViewChildrenDirection::Row,
        children: vec![background, pip],
        background_color: RGBAColor(128, 128, 128, 255),
        ..Default::default()
    })
}

pub fn four_video_layout(ctx: &SceneContext, output_id: &OutputId) -> Component {
    let (output_index, _) = ctx
        .outputs
        .iter()
        .enumerate()
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }

    let input_1 = ctx.inputs[(output_index * 4) % ctx.inputs.len()].clone();
    let input_2 = ctx.inputs[(output_index * 4 + 1) % ctx.inputs.len()].clone();
    let input_3 = ctx.inputs[(output_index * 4 + 2) % ctx.inputs.len()].clone();
    let input_4 = ctx.inputs[(output_index * 4 + 3) % ctx.inputs.len()].clone();

    Component::Tiles(TilesComponent {
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
        .find(|(_index, (id, _))| id == output_id)
        .unwrap();

    if ctx.inputs.is_empty() {
        return blank(ctx, output_id);
    }
    let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
    Component::InputStream(InputStreamComponent { id: None, input_id })
}
