use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;

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

#[render_test(description = "")]
fn change_rescaler_absolute_and_send_next_update() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_rescaler_absolute_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_rescaler_absolute_end.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_rescaler_absolute_after_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width_and_send_abort_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_end.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_after_end_without_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width_and_send_next_update() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_end.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_after_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_height() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_height_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_height_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_absolute() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_absolute_cubic_bezier() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_cubic_bezier_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_cubic_bezier_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn change_view_absolute_cubic_bezier_linear_like() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_cubic_bezier_linear_like_start.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./transition/change_view_absolute_cubic_bezier_linear_like_end.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(2500));
    runner.snapshot(Duration::from_millis(5000));
    runner.snapshot(Duration::from_millis(7500));
    runner.snapshot(Duration::from_millis(9000));
    runner.snapshot(Duration::from_millis(10000));
    runner.finish()
}

#[render_test(description = "")]
fn update_scene_with_transition_interrupt() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_start.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_end_variant1.scene.json"
    ));
    runner.snapshot(Duration::from_millis(5000));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_end_variant1.scene.json"
    ));
    runner.snapshot(Duration::from_millis(7500));
    runner.finish()
}

#[render_test(description = "")]
fn update_scene_with_transition_interrupt_and_changing_props() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_start.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_end_variant1.scene.json"
    ));
    runner.snapshot(Duration::from_millis(5000));
    runner.update_scene_json(include_str!(
        "./transition/change_view_width_transition_interrupt_end_variant2.scene.json"
    ));
    runner.snapshot(Duration::from_millis(7500));
    runner.finish()
}
