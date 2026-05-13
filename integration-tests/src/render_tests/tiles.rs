use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::Resolution;

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    TILES_01_INPUTS,
    TILES_02_INPUTS,
    TILES_03_INPUTS,
    TILES_04_INPUTS,
    TILES_05_INPUTS,
    TILES_15_INPUTS,
    TILES_01_PORTRAIT_INPUTS,
    TILES_02_PORTRAIT_INPUTS,
    TILES_03_PORTRAIT_INPUTS,
    TILES_05_PORTRAIT_INPUTS,
    TILES_15_PORTRAIT_INPUTS,
    TILES_01_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_03_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_03_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_05_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_15_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    ALIGN_CENTER_WITH_03_INPUTS,
    ALIGN_TOP_LEFT_WITH_03_INPUTS,
    ALIGN_WITH_MARGIN_AND_PADDING_WITH_03_INPUTS,
    MARGIN_WITH_03_INPUTS,
    MARGIN_AND_PADDING_WITH_03_INPUTS,
    PADDING_WITH_03_INPUTS,
    VIDEO_CALL_WITH_LABELS,
];

const PORTRAIT_RESOLUTION: Resolution = Resolution {
    width: 360,
    height: 640,
};

#[render_test(description = "")]
fn tiles_01_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./tiles/01_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_02_inputs() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1), TestInput::new(2)]);
    runner.update_scene_json(include_str!("./tiles/02_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!("./tiles/03_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_04_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!("./tiles/04_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene_json(include_str!("./tiles/05_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
        TestInput::new(6),
        TestInput::new(7),
        TestInput::new(8),
        TestInput::new(9),
        TestInput::new(10),
        TestInput::new(11),
        TestInput::new(12),
        TestInput::new(13),
        TestInput::new(14),
        TestInput::new(15),
    ]);
    runner.update_scene_json(include_str!("./tiles/15_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_01_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_inputs(vec![TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION)]);
    runner.update_scene_json(include_str!("./tiles/01_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_02_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
    ]);
    runner.update_scene_json(include_str!("./tiles/02_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
    ]);
    runner.update_scene_json(include_str!("./tiles/03_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(4, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(5, PORTRAIT_RESOLUTION),
    ]);
    runner.update_scene_json(include_str!("./tiles/05_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(4, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(5, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(6, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(7, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(8, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(9, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(10, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(11, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(12, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(13, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(14, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(15, PORTRAIT_RESOLUTION),
    ]);
    runner.update_scene_json(include_str!("./tiles/15_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_01_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(vec![TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION)]);
    runner.update_scene_json(include_str!("./tiles/01_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(vec![
            TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
        ]);
    runner.update_scene_json(include_str!("./tiles/03_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(vec![
            TestInput::new(1),
            TestInput::new(2),
            TestInput::new(3),
        ]);
    runner.update_scene_json(include_str!("./tiles/03_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(vec![
            TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(4, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(5, PORTRAIT_RESOLUTION),
        ]);
    runner.update_scene_json(include_str!("./tiles/05_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(vec![
            TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(4, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(5, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(6, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(7, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(8, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(9, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(10, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(11, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(12, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(13, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(14, PORTRAIT_RESOLUTION),
            TestInput::new_with_resolution(15, PORTRAIT_RESOLUTION),
        ]);
    runner.update_scene_json(include_str!("./tiles/15_portrait_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_center_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles/align_center_with_03_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_top_left_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles/align_top_left_with_03_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_with_margin_and_padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles/align_with_margin_and_padding_with_03_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn margin_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!("./tiles/margin_with_03_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn margin_and_padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles/margin_and_padding_with_03_inputs.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!("./tiles/padding_with_03_inputs.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn video_call_with_labels() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new_with_resolution(1, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(2, PORTRAIT_RESOLUTION),
        TestInput::new_with_resolution(3, PORTRAIT_RESOLUTION),
    ]);
    runner.update_scene_json(include_str!("./tiles/video_call_with_labels.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
