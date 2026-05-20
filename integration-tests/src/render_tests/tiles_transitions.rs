use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    TILE_RESIZE_ENTIRE_COMPONENT_WITH_PARENT_TRANSITION,
    TILE_RESIZE_ENTIRE_COMPONENT_WITHOUT_PARENT_TRANSITION,
    CHANGE_ORDER_OF_3_INPUTS_WITH_ID,
    REPLACE_COMPONENT_BY_ADDING_ID,
    ADD_2_INPUTS_AT_THE_END_TO_3_TILES_SCENE,
    ADD_INPUT_ON_2ND_POS_TO_3_TILES_SCENE,
    ADD_INPUT_AT_THE_END_TO_3_TILES_SCENE,
    REPLACE_COMPONENT_BY_CHANGING_ID,
    REPLACE_COMPONENT_BY_CHANGING_ID_AND_ADD_NEW_COMPONENT,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_MARGIN,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_NEW_COMPONENT_LAST_ROW_CENTER_ALIGNED,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_NEW_COMPONENT_LAST_ROW_LEFT_ALIGNED,
];

#[render_test(description = "")]
fn tile_resize_entire_component_with_parent_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_tile_resize.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_tile_resize_with_view_transition.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    // TODO: This transition does not look great, but it would require automatic
    // transitions triggered by a size change (not scene update)
    runner.snapshot(Duration::from_millis(400));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn tile_resize_entire_component_without_parent_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_tile_resize.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_tile_resize.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn change_order_of_3_inputs_with_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_3_inputs_3_id_different_order.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_adding_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_3_inputs_1_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_2_inputs_at_the_end_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_5_inputs_no_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_input_on_2nd_pos_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_4_inputs_1_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_input_at_the_end_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_4_inputs_no_id.scene.json"
    ));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/after_end_with_4_inputs_no_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_3_inputs_3_id_different_component.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_and_add_new_component() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_4_inputs_3_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_margin() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_3_inputs_3_id_different_component_margin.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_new_component_last_row_center_aligned() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id_center.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_4_inputs_3_id_center.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_new_component_last_row_left_aligned() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene_json(include_str!(
        "./tiles_transitions/start_with_3_inputs_all_id_left.scene.json"
    ));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene_json(include_str!(
        "./tiles_transitions/end_with_4_inputs_3_id_left.scene.json"
    ));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}
