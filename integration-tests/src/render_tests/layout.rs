use std::time::Duration;

use anyhow::{Result, ensure};
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, RendererId, RendererSpec, Resolution,
    scene::{
        AbsolutePosition, Component, ComponentId, HorizontalPosition, InputStreamComponent,
        InterpolationKind, Position, RGBAColor, RescalerComponent, ShaderComponent, Size,
        TextComponent, TilesComponent, Transition, VerticalAlign, VerticalPosition,
        ViewChildrenDirection, ViewComponent,
    },
    shader::ShaderSpec,
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    NESTED_LAYOUTS_WITH_MIXED_CHILDREN,
    TILES_WITH_NESTED_CHILDREN_ADD_IN_THE_MIDDLE,
    TILES_UPDATE_DURING_TRANSITION,
    TRANSITIONS_INSIDE_SHADER,
    RESIZE_LAYOUT_ROOT_INSIDE_SHADER,
    INPUT_STREAM_FRAMES_APPEAR_AND_CHANGE_RESOLUTION,
    INDEPENDENT_OUTPUTS_WITH_TRANSITIONS,
    LAYOUT_WITHOUT_SIZE_INSIDE_SHADER_IS_REJECTED,
];

const DARK_GRAY: RGBAColor = RGBAColor(0x33, 0x33, 0x33, 255);
const GRAY: RGBAColor = RGBAColor(0x77, 0x77, 0x77, 255);
const ORANGE: RGBAColor = RGBAColor(255, 165, 0, 255);
const WHITE: RGBAColor = RGBAColor(255, 255, 255, 255);
const MAGENTA: RGBAColor = RGBAColor(255, 0, 255, 255);

const TILES_ID: &str = "tiles";
const PLANE_SHADER_ID: &str = "layout_planes";

fn plane_shader() -> (RendererId, RendererSpec) {
    (
        RendererId(PLANE_SHADER_ID.into()),
        RendererSpec::Shader(ShaderSpec {
            source: include_str!("./shader/layout_planes.wgsl").into(),
        }),
    )
}

fn linear_500ms() -> Transition {
    Transition {
        duration: Duration::from_millis(500),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: false,
    }
}

fn inputs(count: usize) -> Vec<TestInput> {
    (1..=count).map(TestInput::new).collect()
}

/// `input_stream` component for `input_{idx}`, optionally with the matching id.
fn input(idx: usize, with_id: bool) -> Component {
    let name = format!("input_{idx}");
    Component::InputStream(InputStreamComponent {
        id: with_id.then(|| ComponentId(name.clone().into())),
        input_id: InputId(name.into()),
    })
}

fn rescaled_input(idx: usize) -> Component {
    Component::Rescaler(RescalerComponent {
        child: Box::new(input(idx, false)),
        ..Default::default()
    })
}

#[render_test(description = "
    Child textures (inputs and text) placed on different nesting levels of views, rescalers
    and tiles. Every texture has to land in the component it was defined in.
    - Left column: input_1 (green) aligned to the top.
    - Middle column: label, input_2 (yellow) and a row with input_3 (magenta) and
      input_1 (green).
    - Right: tiles with input_4 (blue), input_2 (yellow) and input_3 (magenta).
    - Rotated input_4 (blue) rendered over the bottom right corner.
")]
fn nested_layouts_with_mixed_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(4));
    runner.update_scene(Component::View(ViewComponent {
        background_color: DARK_GRAY,
        children: vec![
            Component::Rescaler(RescalerComponent {
                position: Position::Static {
                    width: Some(120.0),
                    height: None,
                },
                vertical_align: VerticalAlign::Top,
                child: Box::new(input(1, false)),
                ..Default::default()
            }),
            Component::View(ViewComponent {
                direction: ViewChildrenDirection::Column,
                position: Position::Static {
                    width: Some(200.0),
                    height: None,
                },
                background_color: GRAY,
                children: vec![
                    Component::Text(TextComponent {
                        text: "label".into(),
                        font_size: 30.0,
                        line_height: 30.0,
                        background_color: ORANGE,
                        ..Default::default()
                    }),
                    rescaled_input(2),
                    Component::View(ViewComponent {
                        children: vec![rescaled_input(3), rescaled_input(1)],
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            }),
            Component::Tiles(TilesComponent {
                children: vec![
                    input(4, false),
                    Component::View(ViewComponent {
                        background_color: ORANGE,
                        children: vec![rescaled_input(2)],
                        ..Default::default()
                    }),
                    input(3, false),
                ],
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(160.0),
                    height: Some(90.0),
                    position_horizontal: HorizontalPosition::RightOffset(20.0),
                    position_vertical: VerticalPosition::BottomOffset(20.0),
                    rotation_degrees: 30.0,
                }),
                child: Box::new(input(4, false)),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "
    Tiles where each tile is a view with 2 textures: main input and a small overlay with the
    next input. New tile is added on 2nd position, so it stays hidden until the end of the
    transition. Textures of tiles defined after the hidden one have to stay correct.
")]
fn tiles_with_nested_children_add_in_the_middle() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(4));
    let tile = |idx: usize, with_id: bool| {
        Component::View(ViewComponent {
            id: with_id.then(|| ComponentId(format!("tile_{idx}").into())),
            background_color: GRAY,
            children: vec![
                rescaled_input(idx),
                Component::Rescaler(RescalerComponent {
                    position: Position::Absolute(AbsolutePosition {
                        width: Some(64.0),
                        height: Some(36.0),
                        position_horizontal: HorizontalPosition::LeftOffset(0.0),
                        position_vertical: VerticalPosition::BottomOffset(0.0),
                        rotation_degrees: 0.0,
                    }),
                    child: Box::new(input(idx % 4 + 1, false)),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![tile(1, false), tile(2, false), tile(3, false)],
        ..Default::default()
    }));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms()),
        children: vec![
            tile(1, false),
            tile(4, true),
            tile(2, false),
            tile(3, false),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "
    Tiles updated again in the middle of a transition. The second transition has to start
    from positions that were rendered at the time of that update.
")]
fn tiles_update_during_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(4));
    let tiles = |order: &[usize], transition: Option<Transition>| {
        Component::Tiles(TilesComponent {
            id: Some(ComponentId(TILES_ID.into())),
            transition,
            children: order.iter().map(|idx| input(*idx, true)).collect(),
            ..Default::default()
        })
    };
    runner.update_scene(tiles(&[1, 2, 3], None));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(tiles(&[3, 1, 2], Some(linear_500ms())));
    runner.snapshot(Duration::from_millis(250));
    runner.update_scene(tiles(&[2, 3, 1, 4], Some(linear_500ms())));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.snapshot(Duration::from_millis(750));
    runner.finish()
}

#[render_test(description = "
    Layouts with transitions rendered as children of a shader. The shader stretches its first
    child on the top left quarter and the second one on the top right quarter.
    - Top left: tiles changing order of inputs.
    - Top right: orange view with input_4 (blue) shrinking from full to half width, rest
      is white.
    - Bottom: magenta strip from the root view.
")]
fn transitions_inside_shader() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![plane_shader()])
        .with_inputs(inputs(4));
    let scene = |order: &[usize], width: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            direction: ViewChildrenDirection::Column,
            children: vec![
                Component::Shader(ShaderComponent {
                    id: None,
                    shader_id: RendererId(PLANE_SHADER_ID.into()),
                    shader_param: None,
                    size: Size {
                        width: 640.0,
                        height: 270.0,
                    },
                    children: vec![
                        Component::Tiles(TilesComponent {
                            id: Some(ComponentId(TILES_ID.into())),
                            width: Some(320.0),
                            height: Some(180.0),
                            background_color: DARK_GRAY,
                            transition,
                            children: order.iter().map(|idx| input(*idx, true)).collect(),
                            ..Default::default()
                        }),
                        Component::View(ViewComponent {
                            position: Position::Static {
                                width: Some(320.0),
                                height: Some(180.0),
                            },
                            background_color: WHITE,
                            children: vec![Component::View(ViewComponent {
                                id: Some(ComponentId("resized".into())),
                                position: Position::Static {
                                    width: Some(width),
                                    height: None,
                                },
                                background_color: ORANGE,
                                transition,
                                children: vec![rescaled_input(4)],
                                ..Default::default()
                            })],
                            ..Default::default()
                        }),
                    ],
                }),
                Component::View(ViewComponent {
                    background_color: MAGENTA,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(&[1, 2, 3], 320.0, None));
    runner.update_scene(scene(&[3, 1, 2], 160.0, Some(linear_500ms())));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(250));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "
    View that is a direct child of a shader changes its own size with a transition. The shader
    stretches that view on the top left quarter of the output, so the content should look
    the same on every frame, only its resolution changes.
")]
fn resize_layout_root_inside_shader() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![plane_shader()])
        .with_inputs(inputs(1));
    let scene = |width: f32, height: f32, transition: Option<Transition>| {
        Component::Shader(ShaderComponent {
            id: None,
            shader_id: RendererId(PLANE_SHADER_ID.into()),
            shader_param: None,
            size: Size {
                width: 640.0,
                height: 360.0,
            },
            children: vec![Component::View(ViewComponent {
                id: Some(ComponentId("resized".into())),
                position: Position::Static {
                    width: Some(width),
                    height: Some(height),
                },
                background_color: ORANGE,
                transition,
                children: vec![rescaled_input(1)],
                ..Default::default()
            })],
        })
    };
    runner.update_scene(scene(320.0, 180.0, None));
    runner.update_scene(scene(160.0, 90.0, Some(linear_500ms())));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(250));
    runner.snapshot(Duration::from_millis(500));
    runner.snapshot(Duration::from_millis(1000));
    runner.finish()
}

#[render_test(description = "
    Size of an input stream is only known when its frame is rendered, so layout has to follow
    it without a scene update.
    - 0ms: input_1 does not have any frames. White strip is at the top of the left column and
      the rescaler with input_1 is empty.
    - 100ms: input_1 is 160x90. White strip moves below it.
    - 200ms: input_1 is 90x160. White strip moves lower and rescaler fits a portrait input.
    Orange view and input_2 (yellow) should stay in the same place on all frames.
")]
fn input_stream_frames_appear_and_change_resolution() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(2));
    runner.update_scene(Component::View(ViewComponent {
        background_color: DARK_GRAY,
        children: vec![
            Component::View(ViewComponent {
                direction: ViewChildrenDirection::Column,
                position: Position::Static {
                    width: Some(160.0),
                    height: None,
                },
                background_color: GRAY,
                children: vec![
                    input(1, false),
                    Component::View(ViewComponent {
                        position: Position::Static {
                            width: None,
                            height: Some(40.0),
                        },
                        background_color: WHITE,
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            }),
            Component::View(ViewComponent {
                position: Position::Static {
                    width: Some(80.0),
                    height: None,
                },
                background_color: ORANGE,
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Static {
                    width: Some(200.0),
                    height: None,
                },
                child: Box::new(input(1, false)),
                ..Default::default()
            }),
            rescaled_input(2),
        ],
        ..Default::default()
    }));

    runner.set_inputs(vec![TestInput::new(2)]);
    runner.snapshot(Duration::from_millis(0));

    let landscape = Resolution {
        width: 160,
        height: 90,
    };
    runner.set_inputs(vec![
        TestInput::new_with_resolution(1, landscape),
        TestInput::new(2),
    ]);
    runner.snapshot(Duration::from_millis(100));

    let portrait = Resolution {
        width: 90,
        height: 160,
    };
    runner.set_inputs(vec![
        TestInput::new_with_resolution(1, portrait),
        TestInput::new(2),
    ]);
    runner.snapshot(Duration::from_millis(200));
    runner.finish()
}

#[render_test(description = "
    Scene updates of a second output in the middle of a transition on the main output.
    The transition on the main output should not be affected.
")]
fn independent_outputs_with_transitions() -> Result<()> {
    const SECOND_OUTPUT_ID: &str = "output_2";

    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    let scene = |order: &[usize], width: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            background_color: DARK_GRAY,
            children: vec![
                Component::View(ViewComponent {
                    id: Some(ComponentId("resized".into())),
                    position: Position::Static {
                        width: Some(width),
                        height: None,
                    },
                    background_color: ORANGE,
                    transition,
                    ..Default::default()
                }),
                Component::Tiles(TilesComponent {
                    id: Some(ComponentId(TILES_ID.into())),
                    transition,
                    children: order.iter().map(|idx| input(*idx, true)).collect(),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(&[1, 2, 3], 100.0, None));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(scene(&[3, 1, 2], 300.0, Some(linear_500ms())));
    runner.update_scene_for_output(SECOND_OUTPUT_ID, scene(&[1, 2], 50.0, None));
    runner.snapshot(Duration::from_millis(250));
    runner.update_scene_for_output(
        SECOND_OUTPUT_ID,
        scene(&[2, 1, 3], 200.0, Some(linear_500ms())),
    );
    runner.snapshot(Duration::from_millis(400));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "
    View without known width and height can't be a direct child of a shader. That scene update
    should be rejected and the previous scene should still be rendered.
")]
fn layout_without_size_inside_shader_is_rejected() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_renderers(vec![plane_shader()]);
    let scene = |position: Position, background_color: RGBAColor| {
        Component::Shader(ShaderComponent {
            id: None,
            shader_id: RendererId(PLANE_SHADER_ID.into()),
            shader_param: None,
            size: Size {
                width: 640.0,
                height: 360.0,
            },
            children: vec![Component::View(ViewComponent {
                position,
                background_color,
                ..Default::default()
            })],
        })
    };
    let sized = Position::Static {
        width: Some(320.0),
        height: Some(180.0),
    };
    let unsized_width = Position::Static {
        width: None,
        height: Some(180.0),
    };
    runner.update_scene(scene(sized, ORANGE));
    runner.snapshot(Duration::from_millis(0));

    let result = runner.try_update_scene(scene(unsized_width, MAGENTA));
    ensure!(
        result.is_err(),
        "Scene update with unsized layout inside a shader should fail"
    );
    runner.snapshot(Duration::from_millis(100));
    runner.finish()
}
