use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::scene::{
    AbsolutePosition, Component, ComponentId, HorizontalPosition, InterpolationKind, Position,
    RGBAColor, RescalerComponent, Transition, VerticalPosition, ViewChildrenDirection,
    ViewComponent,
};

use crate::render_tests::{RenderTest, harness::test_case::TestRunner};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    CHANGE_RESCALER_ABSOLUTE_AND_SEND_NEXT_UPDATE,
    CHANGE_VIEW_WIDTH_AND_SEND_ABORT_TRANSITION,
    CHANGE_VIEW_WIDTH_AND_SEND_NEXT_UPDATE,
    CHANGE_VIEW_WIDTH,
    CHANGE_VIEW_HEIGHT,
    CHANGE_VIEW_ABSOLUTE,
    CHANGE_VIEW_ABSOLUTE_CUBIC_BEZIER,
    CHANGE_VIEW_ABSOLUTE_CUBIC_BEZIER_LINEAR_LIKE,
    UPDATE_SCENE_WITH_TRANSITION_INTERRUPT,
    UPDATE_SCENE_WITH_TRANSITION_INTERRUPT_AND_CHANGING_PROPS,
];

const RED: RGBAColor = RGBAColor(255, 0, 0, 255);
const GREEN_FULL: RGBAColor = RGBAColor(0, 255, 0, 255);
const GREEN_NAMED: RGBAColor = RGBAColor(0, 128, 0, 255);
const BLUE: RGBAColor = RGBAColor(0, 0, 255, 255);
const MAGENTA: RGBAColor = RGBAColor(255, 0, 255, 255);

const RESIZE_1: &str = "resize_1";
const RESIZE_2: &str = "resize_2";

fn linear_transition_10s() -> Transition {
    Transition {
        duration: Duration::from_secs(10),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: false,
    }
}

fn snapshot_long_transition(runner: &mut TestRunner) {
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
}

#[render_test(description = "")]
fn change_rescaler_absolute_and_send_next_update() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);

    let rescaler =
        |width: f32, height: f32, top: f32, right: f32, transition: Option<Transition>| {
            Component::View(ViewComponent {
                children: vec![Component::Rescaler(RescalerComponent {
                    id: Some(ComponentId(RESIZE_1.into())),
                    position: Position::Absolute(AbsolutePosition {
                        width: Some(width),
                        height: Some(height),
                        position_horizontal: HorizontalPosition::RightOffset(right),
                        position_vertical: VerticalPosition::TopOffset(top),
                        rotation_degrees: 0.0,
                    }),
                    transition,
                    child: Box::new(Component::View(ViewComponent {
                        background_color: GREEN_FULL,
                        ..Default::default()
                    })),
                    ..Default::default()
                })],
                ..Default::default()
            })
        };

    runner.update_scene(rescaler(200.0, 200.0, 20.0, 20.0, None));
    runner.update_scene(rescaler(
        640.0,
        360.0,
        0.0,
        0.0,
        Some(linear_transition_10s()),
    ));
    runner.update_scene(rescaler(640.0, 360.0, 0.0, 0.0, None));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width_and_send_abort_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let scene = |id: Option<&str>, width: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            children: vec![
                Component::View(ViewComponent {
                    background_color: RED,
                    position: Position::Static {
                        width: Some(50.0),
                        height: None,
                    },
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    id: id.map(|s| ComponentId(s.into())),
                    background_color: GREEN_FULL,
                    position: Position::Static {
                        width: Some(width),
                        height: None,
                    },
                    transition,
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(Some(RESIZE_1), 50.0, None));
    runner.update_scene(scene(Some(RESIZE_1), 250.0, Some(linear_transition_10s())));
    // The "without_id" variant — middle view has no id, transition aborts.
    runner.update_scene(scene(None, 250.0, None));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width_and_send_next_update() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let scene = |width: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            children: vec![
                Component::View(ViewComponent {
                    background_color: RED,
                    position: Position::Static {
                        width: Some(50.0),
                        height: None,
                    },
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    id: Some(ComponentId(RESIZE_1.into())),
                    background_color: GREEN_FULL,
                    position: Position::Static {
                        width: Some(width),
                        height: None,
                    },
                    transition,
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(50.0, None));
    runner.update_scene(scene(250.0, Some(linear_transition_10s())));
    runner.update_scene(scene(250.0, None));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let scene = |width: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            children: vec![
                Component::View(ViewComponent {
                    background_color: RED,
                    position: Position::Static {
                        width: Some(50.0),
                        height: None,
                    },
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    id: Some(ComponentId(RESIZE_1.into())),
                    background_color: GREEN_FULL,
                    position: Position::Static {
                        width: Some(width),
                        height: None,
                    },
                    transition,
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(50.0, None));
    runner.update_scene(scene(250.0, Some(linear_transition_10s())));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_height() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let scene = |height: f32, transition: Option<Transition>| {
        Component::View(ViewComponent {
            children: vec![
                Component::View(ViewComponent {
                    background_color: RED,
                    position: Position::Static {
                        width: Some(50.0),
                        height: None,
                    },
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    id: Some(ComponentId(RESIZE_1.into())),
                    background_color: GREEN_FULL,
                    position: Position::Static {
                        width: Some(250.0),
                        height: Some(height),
                    },
                    transition,
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(scene(100.0, None));
    runner.update_scene(scene(200.0, Some(linear_transition_10s())));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

/// Outer `View { children: [absolutely positioned green child] }`.
fn absolute_view(
    width: f32,
    height: f32,
    top: f32,
    right: f32,
    transition: Option<Transition>,
) -> Component {
    Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            id: Some(ComponentId(RESIZE_1.into())),
            background_color: GREEN_FULL,
            position: Position::Absolute(AbsolutePosition {
                width: Some(width),
                height: Some(height),
                position_horizontal: HorizontalPosition::RightOffset(right),
                position_vertical: VerticalPosition::TopOffset(top),
                rotation_degrees: 0.0,
            }),
            transition,
            ..Default::default()
        })],
        ..Default::default()
    })
}

#[render_test(description = "")]
fn change_view_absolute() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(absolute_view(200.0, 200.0, 20.0, 20.0, None));
    runner.update_scene(absolute_view(
        640.0,
        360.0,
        0.0,
        0.0,
        Some(linear_transition_10s()),
    ));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_absolute_cubic_bezier() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(absolute_view(200.0, 200.0, 0.0, 0.0, None));
    runner.update_scene(absolute_view(
        200.0,
        200.0,
        0.0,
        440.0,
        Some(Transition {
            duration: Duration::from_secs(5),
            interpolation_kind: InterpolationKind::CubicBezier {
                x1: 0.83,
                y1: 0.4,
                x2: 0.17,
                y2: 1.0,
            },
            should_interrupt: false,
        }),
    ));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

#[render_test(description = "")]
fn change_view_absolute_cubic_bezier_linear_like() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(absolute_view(200.0, 200.0, 0.0, 0.0, None));
    runner.update_scene(absolute_view(
        200.0,
        200.0,
        0.0,
        440.0,
        Some(Transition {
            duration: Duration::from_secs(5),
            interpolation_kind: InterpolationKind::CubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
            should_interrupt: false,
        }),
    ));
    snapshot_long_transition(&mut runner);
    runner.finish()
}

/// Two-column scene used by the interrupt tests: each column has a colored static-width view
/// with a transition followed by a blue filler.
fn interrupt_scene(
    width: f32,
    height: Option<f32>,
    resize_1_transition: Transition,
    resize_2_transition: Transition,
) -> Component {
    let row = |id: &str, color: RGBAColor, transition: Transition| {
        Component::View(ViewComponent {
            children: vec![
                Component::View(ViewComponent {
                    id: Some(ComponentId(id.into())),
                    background_color: color,
                    position: Position::Static {
                        width: Some(width),
                        height,
                    },
                    transition: Some(transition),
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    background_color: BLUE,
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    Component::View(ViewComponent {
        direction: ViewChildrenDirection::Column,
        children: vec![
            row(RESIZE_1, GREEN_NAMED, resize_1_transition),
            row(RESIZE_2, MAGENTA, resize_2_transition),
        ],
        ..Default::default()
    })
}

#[render_test(description = "")]
fn update_scene_with_transition_interrupt() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let non_interrupting = Transition {
        duration: Duration::from_secs(10),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: false,
    };
    let interrupting = Transition {
        duration: Duration::from_secs(10),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: true,
    };
    runner.update_scene(interrupt_scene(50.0, None, non_interrupting, interrupting));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(interrupt_scene(640.0, None, non_interrupting, interrupting));
    runner.snapshot(Duration::from_millis(5000));
    runner.update_scene(interrupt_scene(640.0, None, non_interrupting, interrupting));
    runner.snapshot(Duration::from_millis(7500));
    runner.finish()
}

#[render_test(description = "")]
fn update_scene_with_transition_interrupt_and_changing_props() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    let non_interrupting = Transition {
        duration: Duration::from_secs(10),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: false,
    };
    let interrupting = Transition {
        duration: Duration::from_secs(10),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt: true,
    };
    runner.update_scene(interrupt_scene(50.0, None, non_interrupting, interrupting));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(interrupt_scene(640.0, None, non_interrupting, interrupting));
    runner.snapshot(Duration::from_millis(5000));
    // Variant 2 adds a fixed height of 150.
    runner.update_scene(interrupt_scene(
        640.0,
        Some(150.0),
        non_interrupting,
        interrupting,
    ));
    runner.snapshot(Duration::from_millis(7500));
    runner.finish()
}
