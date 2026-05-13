use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, RendererId, RendererSpec,
    scene::{
        Component, InputStreamComponent, ShaderComponent, ShaderParam, ShaderParamStructField,
    },
    shader::ShaderSpec,
};

use crate::render_tests::{
    RenderTest,
    harness::{DEFAULT_RESOLUTION, input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    BASE_PARAMS_PLANE_ID_NO_INPUTS,
    BASE_PARAMS_PLANE_ID_5_INPUTS,
    BASE_PARAMS_TIME,
    BASE_PARAMS_OUTPUT_RESOLUTION,
    BASE_PARAMS_TEXTURE_COUNT_NO_INPUTS,
    BASE_PARAMS_TEXTURE_COUNT_1_INPUT,
    BASE_PARAMS_TEXTURE_COUNT_2_INPUTS,
    USER_PARAMS_CIRCLE_LAYOUT,
];

fn plane_id_shader() -> (RendererId, RendererSpec) {
    (
        RendererId("base_params_plane_id".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./shader/layout_planes.wgsl").into(),
        }),
    )
}

fn time_shader() -> (RendererId, RendererSpec) {
    (
        RendererId("base_params_time".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./shader/fade_to_ball.wgsl").into(),
        }),
    )
}

fn texture_count_shader() -> (RendererId, RendererSpec) {
    (
        RendererId("base_params_texture_count".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./shader/color_output_with_texture_count.wgsl").into(),
        }),
    )
}

fn output_resolution_shader() -> (RendererId, RendererSpec) {
    (
        RendererId("base_params_output_resolution".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./shader/red_border.wgsl").into(),
        }),
    )
}

#[render_test(description = "")]
fn base_params_plane_id_no_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_renderers(vec![plane_id_shader()]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_plane_id_no_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn base_params_plane_id_5_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![plane_id_shader()])
        .with_inputs(vec![
            TestInput::new(1),
            TestInput::new(2),
            TestInput::new(3),
            TestInput::new(4),
            TestInput::new(5),
        ]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_plane_id_5_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn base_params_time() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![time_shader()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./shader/base_params_time.scene.json"));
    runner.snapshot(Duration::from_secs(0));
    runner.snapshot(Duration::from_secs(1));
    runner.snapshot(Duration::from_secs(2));
    runner.finish()
}

#[render_test(description = "")]
fn base_params_output_resolution() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![output_resolution_shader()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_output_resolution.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn base_params_texture_count_no_inputs() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_renderers(vec![texture_count_shader()]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_texture_count_no_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn base_params_texture_count_1_input() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![texture_count_shader()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_texture_count_1_input.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn base_params_texture_count_2_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![texture_count_shader()])
        .with_inputs(vec![TestInput::new(1), TestInput::new(2)]);
    runner.update_scene_json(include_str!(
        "./shader/base_params_texture_count_2_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

struct CircleLayout {
    pub left_px: u32,
    pub top_px: u32,
    pub width_px: u32,
    pub height_px: u32,
    /// RGBA 0.0 - 1.0 range
    pub background_color: [f32; 4],
}

impl CircleLayout {
    pub fn shader_param(&self) -> ShaderParam {
        ShaderParam::Struct(vec![
            ShaderParamStructField {
                field_name: "left_px".to_string(),
                value: ShaderParam::U32(self.left_px),
            },
            ShaderParamStructField {
                field_name: "top_px".to_string(),
                value: ShaderParam::U32(self.top_px),
            },
            ShaderParamStructField {
                field_name: "width_px".to_string(),
                value: ShaderParam::U32(self.width_px),
            },
            ShaderParamStructField {
                field_name: "height_px".to_string(),
                value: ShaderParam::U32(self.height_px),
            },
            ShaderParamStructField {
                field_name: "background_color".to_string(),
                value: ShaderParam::List(vec![
                    ShaderParam::F32(self.background_color[0]),
                    ShaderParam::F32(self.background_color[1]),
                    ShaderParam::F32(self.background_color[2]),
                    ShaderParam::F32(self.background_color[3]),
                ]),
            },
        ])
    }
}

fn circle_layout_scene() -> (Vec<TestInput>, Component) {
    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    let shader_id = RendererId("user_params_circle_layout".into());

    let input1 = TestInput::new(1);
    let input2 = TestInput::new(2);
    let input3 = TestInput::new(3);
    let input4 = TestInput::new(4);

    let layout1 = CircleLayout {
        left_px: 0,
        top_px: 0,
        width_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        height_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        background_color: RED,
    };

    let layout2 = CircleLayout {
        left_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        top_px: 0,
        width_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        height_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        background_color: GREEN,
    };

    let layout3 = CircleLayout {
        left_px: 0,
        top_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        width_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        height_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        background_color: BLUE,
    };

    let layout4 = CircleLayout {
        left_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        top_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        width_px: (DEFAULT_RESOLUTION.width / 2) as u32,
        height_px: (DEFAULT_RESOLUTION.height / 2) as u32,
        background_color: WHITE,
    };

    let scene = Component::Shader(ShaderComponent {
        id: None,
        shader_id,
        shader_param: Some(ShaderParam::List(vec![
            layout1.shader_param(),
            layout2.shader_param(),
            layout3.shader_param(),
            layout4.shader_param(),
        ])),
        size: DEFAULT_RESOLUTION.into(),
        children: vec![
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: InputId(input1.name.clone().into()),
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: InputId(input2.name.clone().into()),
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: InputId(input3.name.clone().into()),
            }),
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: InputId(input4.name.clone().into()),
            }),
        ],
    });

    (vec![input1, input2, input3, input4], scene)
}

#[render_test(description = "")]
fn user_params_circle_layout() -> Result<()> {
    let (inputs, scene) = circle_layout_scene();
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![(
            RendererId("user_params_circle_layout".into()),
            RendererSpec::Shader(ShaderSpec {
                source: include_str!("./shader/circle_layout.wgsl").into(),
            }),
        )])
        .with_inputs(inputs);
    runner.update_scene(scene);
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
