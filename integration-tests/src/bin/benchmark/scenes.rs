use smelter_core::{AudioMixerConfig, AudioMixerInputConfig};
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

pub type BuilderFn =
    fn(ctx: &SceneContext, output_id: &OutputId) -> (Component, AudioMixerConfig);

#[derive(Clone, Copy)]
pub struct SceneLayout {
    pub label: &'static str,
    pub builder: BuilderFn,
    pub inputs: Count,
    pub outputs: Count,
    pub resources: fn() -> Vec<(RendererId, RendererSpec)>,
}

#[derive(Clone, Copy)]
pub enum Count {
    Fixed(u64),
    Scaled(u64),
}

impl Count {
    pub fn eval(self, value: u64) -> u64 {
        match self {
            Count::Fixed(n) => n,
            Count::Scaled(n) => n * value,
        }
    }
}

// N inputs -> N outputs: each output shows 1 distinct input.
pub const SINGLE_VIDEO_N_TO_N: SceneLayout = SceneLayout {
    label: "single_video_n_to_n",
    builder: |ctx, output_id| {
        let output_index = ctx.outputs.iter().position(|id| id == output_id).unwrap_or(0);
        if ctx.inputs.is_empty() {
            return (
                Component::View(ViewComponent {
                    background_color: RGBAColor(128, 128, 128, 255),
                    ..Default::default()
                }),
                AudioMixerConfig { inputs: vec![] },
            );
        }
        let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
        (
            Component::Tiles(TilesComponent {
                margin: 2.0,
                children: vec![Component::InputStream(InputStreamComponent {
                    id: None,
                    input_id: input_id.clone(),
                })],
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig {
                inputs: vec![AudioMixerInputConfig { input_id, volume: 1.0 }],
            },
        )
    },
    inputs: Count::Scaled(1),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 2N inputs -> N outputs: each output tiles 2 inputs.
pub const TWO_VIDEO_2N_TO_N: SceneLayout = SceneLayout {
    label: "two_video_2n_to_n",
    builder: |ctx, output_id| {
        let output_index = ctx.outputs.iter().position(|id| id == output_id).unwrap_or(0);
        if ctx.inputs.is_empty() {
            return (
                Component::View(ViewComponent {
                    background_color: RGBAColor(128, 128, 128, 255),
                    ..Default::default()
                }),
                AudioMixerConfig { inputs: vec![] },
            );
        }
        let input_1 = ctx.inputs[(output_index * 2) % ctx.inputs.len()].clone();
        let input_2 = ctx.inputs[(output_index * 2 + 1) % ctx.inputs.len()].clone();
        (
            Component::Tiles(TilesComponent {
                margin: 2.0,
                children: vec![
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_1.clone(),
                    }),
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_2.clone(),
                    }),
                ],
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig {
                inputs: vec![
                    AudioMixerInputConfig { input_id: input_1, volume: 1.0 },
                    AudioMixerInputConfig { input_id: input_2, volume: 1.0 },
                ],
            },
        )
    },
    inputs: Count::Scaled(2),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 4N inputs -> N outputs: each output tiles 4 inputs.
pub const FOUR_VIDEO_4N_TO_N: SceneLayout = SceneLayout {
    label: "four_video_4n_to_n",
    builder: |ctx, output_id| {
        let output_index = ctx.outputs.iter().position(|id| id == output_id).unwrap_or(0);
        if ctx.inputs.is_empty() {
            return (
                Component::View(ViewComponent {
                    background_color: RGBAColor(128, 128, 128, 255),
                    ..Default::default()
                }),
                AudioMixerConfig { inputs: vec![] },
            );
        }
        let input_1 = ctx.inputs[(output_index * 4) % ctx.inputs.len()].clone();
        let input_2 = ctx.inputs[(output_index * 4 + 1) % ctx.inputs.len()].clone();
        let input_3 = ctx.inputs[(output_index * 4 + 2) % ctx.inputs.len()].clone();
        let input_4 = ctx.inputs[(output_index * 4 + 3) % ctx.inputs.len()].clone();
        (
            Component::Tiles(TilesComponent {
                margin: 2.0,
                children: vec![
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_1.clone(),
                    }),
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_2.clone(),
                    }),
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_3.clone(),
                    }),
                    Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: input_4.clone(),
                    }),
                ],
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig {
                inputs: vec![
                    AudioMixerInputConfig { input_id: input_1, volume: 1.0 },
                    AudioMixerInputConfig { input_id: input_2, volume: 1.0 },
                    AudioMixerInputConfig { input_id: input_3, volume: 1.0 },
                    AudioMixerInputConfig { input_id: input_4, volume: 1.0 },
                ],
            },
        )
    },
    inputs: Count::Scaled(4),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 1 input -> N outputs: every output renders the same single input.
pub const SINGLE_VIDEO_1_TO_N: SceneLayout = SceneLayout {
    label: "single_video_1_to_n",
    builder: |ctx, output_id| {
        let output_index = ctx.outputs.iter().position(|id| id == output_id).unwrap_or(0);
        if ctx.inputs.is_empty() {
            return (
                Component::View(ViewComponent {
                    background_color: RGBAColor(128, 128, 128, 255),
                    ..Default::default()
                }),
                AudioMixerConfig { inputs: vec![] },
            );
        }
        let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
        (
            Component::Tiles(TilesComponent {
                margin: 2.0,
                children: vec![Component::InputStream(InputStreamComponent {
                    id: None,
                    input_id: input_id.clone(),
                })],
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig {
                inputs: vec![AudioMixerInputConfig { input_id, volume: 1.0 }],
            },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// N inputs -> 1 output: blank scene (inputs decoded but unused), measures decoders.
pub const BLANK_N_TO_1: SceneLayout = SceneLayout {
    label: "blank_n_to_1",
    builder: |_ctx, _output_id| {
        (
            Component::View(ViewComponent {
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig { inputs: vec![] },
        )
    },
    inputs: Count::Scaled(1),
    outputs: Count::Fixed(1),
    resources: || vec![],
};

// 1 input -> N outputs: blank scene (input unused), measures encoders/output fanout.
pub const BLANK_1_TO_N: SceneLayout = SceneLayout {
    label: "blank_1_to_n",
    builder: |_ctx, _output_id| {
        (
            Component::View(ViewComponent {
                background_color: RGBAColor(128, 128, 128, 255),
                ..Default::default()
            }),
            AudioMixerConfig { inputs: vec![] },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 1 input -> N outputs: every output tiles the single input.
pub const TILES_1_TO_N: SceneLayout = SceneLayout {
    label: "tiles_1_to_n",
    builder: |ctx, _output_id| {
        (
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
            }),
            AudioMixerConfig {
                inputs: ctx
                    .inputs
                    .iter()
                    .map(|input_id| AudioMixerInputConfig {
                        input_id: input_id.clone(),
                        volume: 1.0,
                    })
                    .collect(),
            },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 1 input -> N outputs: passthrough (skips one copy compared to SINGLE_VIDEO_1_TO_N).
pub const PASS_THROUGH_1_TO_N: SceneLayout = SceneLayout {
    label: "pass_through_1_to_n",
    builder: |ctx, output_id| {
        let output_index = ctx.outputs.iter().position(|id| id == output_id).unwrap_or(0);
        if ctx.inputs.is_empty() {
            return (
                Component::View(ViewComponent {
                    background_color: RGBAColor(128, 128, 128, 255),
                    ..Default::default()
                }),
                AudioMixerConfig { inputs: vec![] },
            );
        }
        let input_id = ctx.inputs[output_index % ctx.inputs.len()].clone();
        (
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_id.clone(),
            }),
            AudioMixerConfig {
                inputs: vec![AudioMixerInputConfig { input_id, volume: 1.0 }],
            },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || vec![],
};

// 1 input -> N outputs: every output renders a static image, ignoring the input.
pub const STATIC_IMAGE_1_TO_N: SceneLayout = SceneLayout {
    label: "static_image_1_to_n",
    builder: |_ctx, _output_id| {
        (
            Component::Rescaler(RescalerComponent {
                child: Component::Image(ImageComponent {
                    id: None,
                    image_id: RendererId("example_image".into()),
                    width: None,
                    height: None,
                })
                .into(),
                ..Default::default()
            }),
            AudioMixerConfig { inputs: vec![] },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || {
        vec![(
            RendererId("example_image".into()),
            RendererSpec::Image(ImageSpec {
                src: ImageSource::LocalPath { path: example_image_path().into() },
                image_type: ImageType::Png,
            }),
        )]
    },
};

// 1 input -> N outputs: every output renders an image processed through a shader.
pub const IMAGE_WITH_SHADER_1_TO_N: SceneLayout = SceneLayout {
    label: "image_with_shader_1_to_n",
    builder: |_ctx, _output_id| {
        (
            Component::Shader(ShaderComponent {
                children: vec![Component::Image(ImageComponent {
                    id: None,
                    image_id: RendererId("example_image".into()),
                    width: None,
                    height: None,
                })],
                id: None,
                shader_id: RendererId("example_shader".into()),
                shader_param: None,
                size: Size { width: 1920.0, height: 1080.0 },
            }),
            AudioMixerConfig { inputs: vec![] },
        )
    },
    inputs: Count::Fixed(1),
    outputs: Count::Scaled(1),
    resources: || {
        vec![
            (
                RendererId("example_image".into()),
                RendererSpec::Image(ImageSpec {
                    src: ImageSource::LocalPath { path: example_image_path().into() },
                    image_type: ImageType::Png,
                }),
            ),
            (
                RendererId("example_shader".into()),
                RendererSpec::Shader(ShaderSpec {
                    source: include_str!("./silly.wgsl").into(),
                }),
            ),
        ]
    },
};
