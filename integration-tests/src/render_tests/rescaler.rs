use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, RenderingMode, Resolution,
    scene::{
        AbsolutePosition, BorderRadius, BoxShadow, Component, HorizontalAlign,
        HorizontalPosition, InputStreamComponent, Position, RGBAColor, RescaleMode,
        RescalerComponent, VerticalAlign, VerticalPosition, ViewComponent,
    },
};

use crate::render_tests::{
    RenderTest,
    harness::{DEFAULT_RESOLUTION, input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    FIT_VIEW_WITH_KNOWN_HEIGHT,
    FIT_VIEW_WITH_KNOWN_WIDTH,
    FIT_VIEW_WITH_UNKNOWN_WIDTH_AND_HEIGHT,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_TOP_LEFT,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FILL_INPUT_STREAM_LOWER_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FILL_INPUT_STREAM_LOWER_ASPECT_RATIO,
    FILL_INPUT_STREAM_HIGHER_ASPECT_RATIO,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO,
    FILL_INPUT_STREAM_MATCHING_ASPECT_RATIO,
    FIT_INPUT_STREAM_LOWER_ASPECT_RATIO,
    FIT_INPUT_STREAM_HIGHER_ASPECT_RATIO,
    FIT_INPUT_STREAM_HIGHER_ASPECT_RATIO_SMALL_RESOLUTION,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_TOP_LEFT,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FIT_INPUT_STREAM_LOWER_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO,
    FIT_INPUT_STREAM_MATCHING_ASPECT_RATIO,
    BORDER_RADIUS,
    BORDER_WIDTH,
    BOX_SHADOW,
    BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW_FIT_INPUT_STREAM,
    BORDER_RADIUS_BOX_SHADOW_FILL_INPUT_STREAM,
    NESTED_BORDER_WIDTH_RADIUS,
    NESTED_BORDER_WIDTH_RADIUS_ALIGNED,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED,
    SCALING_FILTER_BILINEAR,
    SCALING_FILTER_LANCZOS3,
];

const RED: RGBAColor = RGBAColor(255, 0, 0, 255);
const GREEN: RGBAColor = RGBAColor(0, 255, 0, 255);
const BLUE: RGBAColor = RGBAColor(0, 0, 255, 255);
const YELLOW: RGBAColor = RGBAColor(255, 255, 0, 255);
const WHITE: RGBAColor = RGBAColor(255, 255, 255, 255);

fn input_stream(id: &str) -> Component {
    Component::InputStream(InputStreamComponent {
        id: None,
        input_id: InputId(id.into()),
    })
}

fn box_shadow_offset_30(color: RGBAColor) -> BoxShadow {
    BoxShadow { offset_x: 60.0, offset_y: 30.0, blur_radius: 30.0, color }
}

#[render_test(description = "")]
fn fit_view_with_known_height() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static { width: Some(160.0), height: Some(90.0) },
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(320.0),
                    height: Some(180.0),
                    position_horizontal: HorizontalPosition::LeftOffset(160.0),
                    position_vertical: VerticalPosition::TopOffset(90.0),
                    rotation_degrees: 0.0,
                }),
                mode: RescaleMode::Fit,
                child: Box::new(Component::View(ViewComponent {
                    background_color: BLUE,
                    position: Position::Static { width: None, height: Some(100.0) },
                    ..Default::default()
                })),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_view_with_known_width() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static { width: Some(160.0), height: Some(90.0) },
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(320.0),
                    height: Some(180.0),
                    position_horizontal: HorizontalPosition::LeftOffset(160.0),
                    position_vertical: VerticalPosition::TopOffset(90.0),
                    rotation_degrees: 0.0,
                }),
                mode: RescaleMode::Fit,
                child: Box::new(Component::View(ViewComponent {
                    background_color: BLUE,
                    position: Position::Static { width: Some(200.0), height: None },
                    ..Default::default()
                })),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_view_with_unknown_width_and_height() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static { width: Some(160.0), height: Some(90.0) },
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(320.0),
                    height: Some(180.0),
                    position_horizontal: HorizontalPosition::LeftOffset(160.0),
                    position_vertical: VerticalPosition::TopOffset(90.0),
                    rotation_degrees: 0.0,
                }),
                mode: RescaleMode::Fit,
                child: Box::new(Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                })),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio_align_top_left() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner
        .update_scene(fill_input_stream_scene(HorizontalAlign::Left, VerticalAlign::Top));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Right,
        VerticalAlign::Bottom,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_lower_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        ),
    ]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Right,
        VerticalAlign::Bottom,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_lower_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        ),
    ]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_higher_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height + 100,
            },
        ),
    ]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_matching_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(fill_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_lower_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        ),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_higher_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height + 100,
            },
        ),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_higher_aspect_ratio_small_resolution() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width / 10,
                height: (DEFAULT_RESOLUTION.height + 100) / 10,
            },
        ),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio_align_top_left() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner
        .update_scene(fit_input_stream_scene(HorizontalAlign::Left, VerticalAlign::Top));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Right,
        VerticalAlign::Bottom,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_lower_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        ),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Right,
        VerticalAlign::Bottom,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, Resolution { width: 360, height: 640 }),
    ]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_matching_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(fit_input_stream_scene(
        HorizontalAlign::Center,
        VerticalAlign::Center,
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

fn fill_input_stream_scene(
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
) -> Component {
    Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static { width: Some(160.0), height: Some(90.0) },
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(320.0),
                    height: Some(180.0),
                    position_horizontal: HorizontalPosition::LeftOffset(160.0),
                    position_vertical: VerticalPosition::TopOffset(90.0),
                    rotation_degrees: 0.0,
                }),
                mode: RescaleMode::Fill,
                horizontal_align,
                vertical_align,
                child: Box::new(input_stream("input_1")),
                ..Default::default()
            }),
        ],
        ..Default::default()
    })
}

fn fit_input_stream_scene(
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
) -> Component {
    Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static { width: Some(160.0), height: Some(90.0) },
                ..Default::default()
            }),
            Component::Rescaler(RescalerComponent {
                position: Position::Absolute(AbsolutePosition {
                    width: Some(320.0),
                    height: Some(180.0),
                    position_horizontal: HorizontalPosition::LeftOffset(160.0),
                    position_vertical: VerticalPosition::TopOffset(90.0),
                    rotation_degrees: 0.0,
                }),
                mode: RescaleMode::Fit,
                horizontal_align,
                vertical_align,
                child: Box::new(input_stream("input_1")),
                ..Default::default()
            }),
        ],
        ..Default::default()
    })
}

#[render_test(description = "")]
fn border_radius() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_width() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_width: 20.0,
            border_color: WHITE,
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            box_shadow: vec![box_shadow_offset_30(GREEN)],
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 20.0,
            border_color: WHITE,
            box_shadow: vec![box_shadow_offset_30(GREEN)],
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            box_shadow: vec![box_shadow_offset_30(GREEN)],
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_fit_input_stream() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 20.0,
            border_color: WHITE,
            mode: RescaleMode::Fit,
            box_shadow: vec![box_shadow_offset_30(GREEN)],
            child: Box::new(input_stream("input_1")),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_fill_input_stream() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 20.0,
            border_color: WHITE,
            mode: RescaleMode::Fill,
            box_shadow: vec![box_shadow_offset_30(GREEN)],
            child: Box::new(input_stream("input_1")),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 20.0,
            border_color: RED,
            child: Box::new(Component::Rescaler(RescalerComponent {
                border_radius: BorderRadius::new_with_radius(50.0),
                border_width: 20.0,
                border_color: GREEN,
                child: Box::new(Component::Rescaler(RescalerComponent {
                    border_radius: BorderRadius::new_with_radius(50.0),
                    border_width: 20.0,
                    border_color: BLUE,
                    mode: RescaleMode::Fill,
                    child: Box::new(input_stream("input_1")),
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_aligned() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(80.0),
            border_width: 20.0,
            border_color: RED,
            child: Box::new(Component::Rescaler(RescalerComponent {
                border_radius: BorderRadius::new_with_radius(60.0),
                border_width: 20.0,
                border_color: GREEN,
                child: Box::new(Component::Rescaler(RescalerComponent {
                    border_radius: BorderRadius::new_with_radius(40.0),
                    border_width: 20.0,
                    border_color: BLUE,
                    mode: RescaleMode::Fill,
                    child: Box::new(input_stream("input_1")),
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled() -> Result<()> {
    // it is supposed to be cut off because of the rescaler that wraps it
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            position: Position::Static { width: Some(600.0), height: Some(300.0) },
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Center,
            child: Box::new(Component::Rescaler(RescalerComponent {
                position: Position::Static { width: Some(200.0), height: Some(200.0) },
                border_radius: BorderRadius::new_with_radius(50.0),
                border_width: 20.0,
                border_color: WHITE,
                box_shadow: vec![BoxShadow {
                    offset_x: 20.0,
                    offset_y: 20.0,
                    blur_radius: 5.0,
                    color: GREEN,
                }],
                child: Box::new(input_stream("input_1")),
                ..Default::default()
            })),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn scaling_filter_bilinear() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_rendering_mode(RenderingMode::CpuOptimized)
        .with_resolution(Resolution { width: 1920, height: 1080 })
        .with_inputs(vec![TestInput::new_multiscale_grid(
            1,
            Resolution { width: 5760, height: 3240 },
        )]);
    runner.update_scene(Component::Rescaler(RescalerComponent {
        mode: RescaleMode::Fit,
        child: Box::new(input_stream("input_1")),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn scaling_filter_lanczos3() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_rendering_mode(RenderingMode::GpuOptimized)
        .with_resolution(Resolution { width: 1920, height: 1080 })
        .with_inputs(vec![TestInput::new_multiscale_grid(
            1,
            Resolution { width: 5760, height: 3240 },
        )]);
    runner.update_scene(Component::Rescaler(RescalerComponent {
        mode: RescaleMode::Fit,
        child: Box::new(input_stream("input_1")),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
