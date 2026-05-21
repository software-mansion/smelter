use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, Resolution,
    scene::{
        AbsolutePosition, BorderRadius, BoxShadow, Component, HorizontalAlign, HorizontalPosition,
        InputStreamComponent, Overflow, Padding, Position, RGBAColor, RescaleMode,
        RescalerComponent, VerticalAlign, VerticalPosition, ViewChildrenDirection, ViewComponent,
    },
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    OVERFLOW_HIDDEN_WITH_INPUT_STREAM_CHILDREN,
    OVERFLOW_HIDDEN_WITH_VIEW_CHILDREN,
    CONSTANT_WIDTH_VIEWS_ROW,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_HIDDEN,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_VISIBLE,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_FIT,
    DYNAMIC_WIDTH_VIEWS_ROW,
    DYNAMIC_AND_CONSTANT_WIDTH_VIEWS_ROW,
    DYNAMIC_AND_CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW,
    CONSTANT_WIDTH_AND_HEIGHT_VIEWS_ROW,
    VIEW_WITH_ABSOLUTE_POSITIONING_PARTIALLY_COVERED_BY_SIBLING,
    VIEW_WITH_ABSOLUTE_POSITIONING_RENDER_OVER_SIBLINGS,
    ROOT_VIEW_WITH_BACKGROUND_COLOR,
    BORDER_RADIUS,
    BORDER_RADIUS_CLIPPING,
    BORDER_RADIUS_CLIPPING_LARGE_BORDER_WIDTH,
    BORDER_WIDTH,
    BOX_SHADOW,
    BOX_SHADOW_SIBLING,
    BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW_OVERFLOW_HIDDEN,
    BORDER_RADIUS_BOX_SHADOW_OVERFLOW_FIT,
    BORDER_RADIUS_BOX_SHADOW_RESCALER_INPUT_STREAM,
    NESTED_BORDER_WIDTH_RADIUS,
    NESTED_BORDER_WIDTH_RADIUS_ALIGNED,
    NESTED_BORDER_WIDTH_RADIUS_MULTI_CHILD,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED,
    ROOT_BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED_AND_HIDDEN_BY_PARENT,
    UNSIZED_VIEW_PADDING_STATIC_CHILDREN,
    VIEW_PADDING_MULTIPLE_CHILDREN,
    NESTED_PADDING_STATIC_CHILDREN,
    NESTED_PADDING_STATIC_CHILDREN_OVERFLOW_VISIBLE,
    PADDING_ABSOLUTE_CHILDREN,
    VIEW_PADDING_OVERFLOW_CHILDREN,
];

const RED: RGBAColor = RGBAColor(255, 0, 0, 255);
const GREEN_FULL: RGBAColor = RGBAColor(0, 255, 0, 255);
const GREEN_NAMED: RGBAColor = RGBAColor(0, 128, 0, 255);
const BLUE: RGBAColor = RGBAColor(0, 0, 255, 255);
const YELLOW: RGBAColor = RGBAColor(255, 255, 0, 255);
const WHITE: RGBAColor = RGBAColor(255, 255, 255, 255);
const CYAN: RGBAColor = RGBAColor(0, 255, 255, 255);
const MAGENTA: RGBAColor = RGBAColor(255, 0, 255, 255);
const ORANGE: RGBAColor = RGBAColor(255, 165, 0, 255);
const GRAY: RGBAColor = RGBAColor(128, 128, 128, 255);
const DARK_YELLOW_1: RGBAColor = RGBAColor(0xBB, 0xBB, 0, 255);
const DARK_YELLOW_2: RGBAColor = RGBAColor(0x88, 0x88, 0, 255);

fn input_stream(id: &str) -> Component {
    Component::InputStream(InputStreamComponent {
        id: None,
        input_id: InputId(id.into()),
    })
}

fn box_shadow_offset_30(color: RGBAColor) -> BoxShadow {
    BoxShadow {
        offset_x: 60.0,
        offset_y: 30.0,
        blur_radius: 30.0,
        color,
    }
}

fn nested_border_view(
    border_radius: f32,
    border_width: f32,
    border_color: RGBAColor,
    child: Component,
) -> Component {
    Component::View(ViewComponent {
        border_radius: BorderRadius::new_with_radius(border_radius),
        border_width,
        border_color,
        children: vec![child],
        ..Default::default()
    })
}

#[render_test(description = "")]
fn overflow_hidden_with_input_stream_children() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 180,
                height: 200,
            },
        )]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(100.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                children: vec![input_stream("input_1"); 3],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn overflow_hidden_with_view_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(100.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                children: vec![
                    Component::View(ViewComponent {
                        background_color: YELLOW,
                        position: Position::Static {
                            width: Some(180.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: DARK_YELLOW_1,
                        position: Position::Static {
                            width: Some(180.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: DARK_YELLOW_2,
                        position: Position::Static {
                            width: Some(180.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(200.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(200.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(200.0),
                    height: None,
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_hidden() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                children: vec![Component::View(ViewComponent {
                    background_color: YELLOW,
                    position: Position::Absolute(AbsolutePosition {
                        width: Some(500.0),
                        height: Some(100.0),
                        position_horizontal: HorizontalPosition::LeftOffset(-100.0),
                        position_vertical: VerticalPosition::TopOffset(100.0),
                        rotation_degrees: 0.0,
                    }),
                    ..Default::default()
                })],
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_visible() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                overflow: Overflow::Visible,
                children: vec![Component::View(ViewComponent {
                    background_color: YELLOW,
                    position: Position::Absolute(AbsolutePosition {
                        width: Some(500.0),
                        height: Some(100.0),
                        position_horizontal: HorizontalPosition::LeftOffset(-100.0),
                        position_vertical: VerticalPosition::TopOffset(100.0),
                        rotation_degrees: 0.0,
                    }),
                    ..Default::default()
                })],
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_fit() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(300.0),
                    height: None,
                },
                overflow: Overflow::Fit,
                children: vec![
                    Component::View(ViewComponent {
                        background_color: CYAN,
                        position: Position::Static {
                            width: Some(200.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: YELLOW,
                        position: Position::Static {
                            width: Some(200.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: MAGENTA,
                        position: Position::Static {
                            width: Some(200.0),
                            height: Some(200.0),
                        },
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: WHITE,
                        position: Position::Absolute(AbsolutePosition {
                            width: Some(300.0),
                            height: Some(50.0),
                            position_horizontal: HorizontalPosition::LeftOffset(50.0),
                            position_vertical: VerticalPosition::TopOffset(50.0),
                            rotation_degrees: 0.0,
                        }),
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_and_constant_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(100.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(100.0),
                    height: None,
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_and_constant_width_views_row_with_overflow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(400.0),
                    height: None,
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(400.0),
                    height: None,
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_and_height_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(200.0),
                    height: Some(300.0),
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Static {
                    width: Some(200.0),
                    height: Some(200.0),
                },
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                position: Position::Static {
                    width: Some(200.0),
                    height: Some(300.0),
                },
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_with_absolute_positioning_partially_covered_by_sibling() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Absolute(AbsolutePosition {
                    width: Some(400.0),
                    height: Some(200.0),
                    position_horizontal: HorizontalPosition::RightOffset(50.0),
                    position_vertical: VerticalPosition::TopOffset(50.0),
                    rotation_degrees: 0.0,
                }),
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_with_absolute_positioning_render_over_siblings() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![
            Component::View(ViewComponent {
                background_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: BLUE,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                background_color: GREEN_FULL,
                position: Position::Absolute(AbsolutePosition {
                    width: Some(400.0),
                    height: Some(200.0),
                    position_horizontal: HorizontalPosition::RightOffset(50.0),
                    position_vertical: VerticalPosition::TopOffset(50.0),
                    rotation_degrees: 0.0,
                }),
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn root_view_with_background_color() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: RED,
        children: vec![Component::View(ViewComponent {
            background_color: GREEN_FULL,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::RightOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_clipping() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(500.0),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_clipping_large_border_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(100.0),
                height: Some(100.0),
                position_horizontal: HorizontalPosition::LeftOffset(25.0),
                position_vertical: VerticalPosition::TopOffset(25.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(500.0),
            border_width: 100.0,
            border_color: BLUE,
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_width: 20.0,
            border_color: WHITE,
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow_sibling() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            background_color: YELLOW,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(100.0),
                position_vertical: VerticalPosition::TopOffset(100.0),
                rotation_degrees: 0.0,
            }),
            overflow: Overflow::Visible,
            children: vec![
                Component::View(ViewComponent {
                    background_color: RED,
                    box_shadow: vec![
                        BoxShadow {
                            offset_x: 0.0,
                            offset_y: 60.0,
                            blur_radius: 30.0,
                            color: RED,
                        },
                        BoxShadow {
                            offset_x: -60.0,
                            offset_y: -30.0,
                            blur_radius: 30.0,
                            color: BLUE,
                        },
                    ],
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: RED,
                    border_width: 20.0,
                    border_color: WHITE,
                    box_shadow: vec![box_shadow_offset_30(BLUE)],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
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
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_overflow_hidden() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
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
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            children: vec![input_stream("input_1")],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_overflow_fit() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            overflow: Overflow::Fit,
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 20.0,
            border_color: WHITE,
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            children: vec![input_stream("input_1")],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_rescaler_input_stream() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            background_color: RED,
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
            box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
            children: vec![Component::Rescaler(RescalerComponent {
                child: Box::new(input_stream("input_1")),
                mode: RescaleMode::Fill,
                vertical_align: VerticalAlign::Top,
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
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
            children: vec![nested_border_view(
                50.0,
                20.0,
                GREEN_FULL,
                nested_border_view(50.0, 20.0, BLUE, input_stream("input_1")),
            )],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_aligned() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
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
            children: vec![nested_border_view(
                60.0,
                20.0,
                GREEN_FULL,
                nested_border_view(40.0, 20.0, BLUE, input_stream("input_1")),
            )],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_multi_child() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    let leaf = || nested_border_view(30.0, 10.0, BLUE, input_stream("input_1"));
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            position: Position::Absolute(AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: HorizontalPosition::LeftOffset(50.0),
                position_vertical: VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: BorderRadius::new_with_radius(50.0),
            border_width: 10.0,
            border_color: RED,
            children: vec![
                nested_border_view(40.0, 10.0, GREEN_FULL, leaf()),
                Component::View(ViewComponent {
                    border_radius: BorderRadius::new_with_radius(40.0),
                    border_width: 10.0,
                    border_color: GREEN_FULL,
                    children: vec![leaf(), leaf()],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::Rescaler(RescalerComponent {
            child: Box::new(Component::View(ViewComponent {
                background_color: RED,
                position: Position::Static {
                    width: Some(200.0),
                    height: Some(200.0),
                },
                border_radius: BorderRadius::new_with_radius(50.0),
                border_width: 20.0,
                border_color: WHITE,
                box_shadow: vec![BoxShadow {
                    offset_x: 20.0,
                    offset_y: 20.0,
                    blur_radius: 5.0,
                    color: GREEN_FULL,
                }],
                ..Default::default()
            })),
            position: Position::Static {
                width: Some(600.0),
                height: Some(300.0),
            },
            horizontal_align: HorizontalAlign::Center,
            vertical_align: VerticalAlign::Center,
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn root_border_radius_border_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: RED,
        border_radius: BorderRadius::new_with_radius(50.0),
        border_width: 20.0,
        border_color: WHITE,
        box_shadow: vec![box_shadow_offset_30(GREEN_FULL)],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled_and_hidden_by_parent() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: YELLOW,
        children: vec![Component::View(ViewComponent {
            position: Position::Static {
                width: Some(460.0),
                height: Some(270.0),
            },
            children: vec![Component::Rescaler(RescalerComponent {
                child: Box::new(Component::View(ViewComponent {
                    background_color: RED,
                    position: Position::Static {
                        width: Some(200.0),
                        height: Some(200.0),
                    },
                    border_radius: BorderRadius::new_with_radius(50.0),
                    border_width: 20.0,
                    border_color: WHITE,
                    box_shadow: vec![BoxShadow {
                        offset_x: 20.0,
                        offset_y: 20.0,
                        blur_radius: 5.0,
                        color: GREEN_FULL,
                    }],
                    ..Default::default()
                })),
                position: Position::Static {
                    width: Some(600.0),
                    height: Some(300.0),
                },
                horizontal_align: HorizontalAlign::Center,
                vertical_align: VerticalAlign::Center,
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn unsized_view_padding_static_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: BLUE,
        direction: ViewChildrenDirection::Column,
        children: vec![
            Component::View(ViewComponent {
                border_width: 10.0,
                border_color: RED,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                padding: Padding {
                    top: 20.0,
                    bottom: 40.0,
                    left: 20.0,
                    right: 20.0,
                },
                border_width: 10.0,
                border_color: RED,
                children: vec![Component::View(ViewComponent {
                    border_width: 10.0,
                    border_color: MAGENTA,
                    background_color: YELLOW,
                    ..Default::default()
                })],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_padding_multiple_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: BLUE,
        children: vec![
            Component::View(ViewComponent::default()),
            Component::View(ViewComponent {
                padding: Padding {
                    top: 0.0,
                    bottom: 20.0,
                    left: 20.0,
                    right: 20.0,
                },
                direction: ViewChildrenDirection::Column,
                background_color: GREEN_NAMED,
                children: vec![
                    Component::View(ViewComponent {
                        background_color: RED,
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        position: Position::Static {
                            width: None,
                            height: Some(250.0),
                        },
                        padding: Padding {
                            top: 20.0,
                            bottom: 20.0,
                            left: 20.0,
                            right: 20.0,
                        },
                        background_color: YELLOW,
                        children: vec![
                            Component::View(ViewComponent {
                                background_color: ORANGE,
                                ..Default::default()
                            }),
                            Component::View(ViewComponent {
                                background_color: GRAY,
                                ..Default::default()
                            }),
                        ],
                        ..Default::default()
                    }),
                    Component::View(ViewComponent {
                        background_color: MAGENTA,
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_padding_static_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: RED,
        children: vec![
            Component::View(ViewComponent {
                border_width: 10.0,
                border_color: BLUE,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                padding: Padding {
                    top: 20.0,
                    bottom: 0.0,
                    left: 20.0,
                    right: 0.0,
                },
                border_width: 10.0,
                border_color: BLUE,
                children: vec![Component::View(ViewComponent {
                    padding: Padding {
                        top: 20.0,
                        bottom: 20.0,
                        left: 20.0,
                        right: 40.0,
                    },
                    border_width: 10.0,
                    border_color: GREEN_NAMED,
                    background_color: BLUE,
                    children: vec![Component::View(ViewComponent {
                        position: Position::Static {
                            width: Some(150.0),
                            height: Some(150.0),
                        },
                        padding: Padding {
                            top: 0.0,
                            bottom: 0.0,
                            left: 80.0,
                            right: 0.0,
                        },
                        border_width: 10.0,
                        border_color: MAGENTA,
                        background_color: YELLOW,
                        ..Default::default()
                    })],
                    ..Default::default()
                })],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_padding_static_children_overflow_visible() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: RED,
        children: vec![
            Component::View(ViewComponent {
                border_width: 10.0,
                border_color: BLUE,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                padding: Padding {
                    top: 20.0,
                    bottom: 0.0,
                    left: 20.0,
                    right: 0.0,
                },
                border_width: 10.0,
                border_color: BLUE,
                children: vec![Component::View(ViewComponent {
                    padding: Padding {
                        top: 20.0,
                        bottom: 20.0,
                        left: 20.0,
                        right: 40.0,
                    },
                    border_width: 10.0,
                    overflow: Overflow::Visible,
                    border_color: GREEN_NAMED,
                    background_color: BLUE,
                    children: vec![Component::View(ViewComponent {
                        position: Position::Static {
                            width: Some(150.0),
                            height: Some(150.0),
                        },
                        padding: Padding {
                            top: 0.0,
                            bottom: 0.0,
                            left: 80.0,
                            right: 0.0,
                        },
                        border_width: 10.0,
                        border_color: MAGENTA,
                        background_color: YELLOW,
                        ..Default::default()
                    })],
                    ..Default::default()
                })],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn padding_absolute_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: RED,
        children: vec![
            Component::View(ViewComponent {
                background_color: BLUE,
                ..Default::default()
            }),
            Component::View(ViewComponent {
                padding: Padding {
                    top: 20.0,
                    bottom: 0.0,
                    left: 20.0,
                    right: 0.0,
                },
                children: vec![Component::View(ViewComponent {
                    background_color: YELLOW,
                    position: Position::Absolute(AbsolutePosition {
                        width: None,
                        height: None,
                        position_horizontal: HorizontalPosition::LeftOffset(40.0),
                        position_vertical: VerticalPosition::TopOffset(40.0),
                        rotation_degrees: 0.0,
                    }),
                    padding: Padding {
                        top: 20.0,
                        bottom: 0.0,
                        left: 20.0,
                        right: 0.0,
                    },
                    children: vec![input_stream("input_1")],
                    ..Default::default()
                })],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_padding_overflow_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        background_color: BLUE,
        direction: ViewChildrenDirection::Column,
        children: vec![Component::View(ViewComponent {
            padding: Padding {
                top: 360.0,
                bottom: 0.0,
                left: 0.0,
                right: 0.0,
            },
            direction: ViewChildrenDirection::Column,
            children: vec![
                Component::View(ViewComponent {
                    background_color: YELLOW,
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: RED,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
