use std::time::Duration;

use smelter_render::{
    scene::{
        Component, InputStreamComponent, ShaderComponent, ShaderParam, ShaderParamStructField,
    },
    shader::ShaderSpec,
    InputId, RendererId, RendererSpec,
};

use super::{test_steps_from_scene, Step, DEFAULT_RESOLUTION};

use super::{input::TestInput, snapshots_path, test_case::TestCase, TestRunner};

#[test]
fn shader_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("shader"));

    let input1 = TestInput::new(1);
    let input2 = TestInput::new(2);
    let input3 = TestInput::new(3);
    let input4 = TestInput::new(4);
    let input5 = TestInput::new(5);

    let plane_id_shader = (
        RendererId("base_params_plane_id".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("../../snapshot_tests/shader/layout_planes.wgsl").into(),
        }),
    );

    let time_shader = (
        RendererId("base_params_time".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("../../snapshot_tests/shader/fade_to_ball.wgsl").into(),
        }),
    );

    let texture_count_shader = (
        RendererId("base_params_texture_count".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!(
                "../../snapshot_tests/shader/color_output_with_texture_count.wgsl"
            )
            .into(),
        }),
    );

    let output_resolution_shader = (
        RendererId("base_params_output_resolution".into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("../../snapshot_tests/shader/red_border.wgsl").into(),
        }),
    );

    runner.add(TestCase {
        name: "shader/base_params_plane_id_no_inputs",
        renderers: vec![plane_id_shader.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_plane_id_no_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_plane_id_5_inputs",
        renderers: vec![plane_id_shader.clone()],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_plane_id_5_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_time",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "../../snapshot_tests/shader/base_params_time.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_secs(0)),
            Step::RenderWithSnapshot(Duration::from_secs(1)),
            Step::RenderWithSnapshot(Duration::from_secs(2)),
        ],
        renderers: vec![time_shader.clone()],
        inputs: vec![input1.clone()],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_output_resolution",
        renderers: vec![output_resolution_shader.clone()],
        inputs: vec![input1.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_output_resolution.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_texture_count_no_inputs",
        renderers: vec![texture_count_shader.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_texture_count_no_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_texture_count_1_input",
        renderers: vec![texture_count_shader.clone()],
        inputs: vec![input1.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_texture_count_1_input.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "shader/base_params_texture_count_2_inputs",
        renderers: vec![texture_count_shader.clone()],
        inputs: vec![input1.clone(), input2.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/shader/base_params_texture_count_2_inputs.scene.json"
        )),
        ..Default::default()
    });

    user_params_snapshot_tests(&mut runner);

    runner.run()
}

fn user_params_snapshot_tests(runner: &mut TestRunner) {
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

    let input1 = TestInput::new(1);
    let input2 = TestInput::new(2);
    let input3 = TestInput::new(3);
    let input4 = TestInput::new(4);

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    let shader_id = RendererId("user_params_circle_layout".into());

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

    let circle_layout_scene = Component::Shader(ShaderComponent {
        id: None,
        shader_id: shader_id.clone(),
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

    runner.add(TestCase {
        name: "shader/user_params_circle_layout",
        renderers: vec![(
            shader_id.clone(),
            RendererSpec::Shader(ShaderSpec {
                source: include_str!("../../snapshot_tests/shader/circle_layout.wgsl").into(),
            }),
        )],
        inputs: vec![input1, input2, input3, input4],
        steps: vec![
            Step::UpdateScene(circle_layout_scene),
            Step::RenderWithSnapshot(Duration::ZERO),
        ],
        ..Default::default()
    });
}
