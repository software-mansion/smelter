use std::time::Duration;

use crate::paths::render_snapshots_dir_path;

use super::{Step, TestRunner, input::TestInput, test_case::TestCase};

#[test]
fn tiles_transitions_tests() {
    let mut runner = TestRunner::new(render_snapshots_dir_path().join("tiles_transitions"));

    let input1 = TestInput::new(1);
    let input2 = TestInput::new(2);
    let input3 = TestInput::new(3);
    let input4 = TestInput::new(4);
    let input5 = TestInput::new(5);

    runner.add(TestCase {
        name: "tiles_transitions/tile_resize_entire_component_with_parent_transition",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_tile_resize.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_tile_resize_with_view_transition.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            // TODO: This transition does not look great, but it would require automatic
            // transitions triggered by a size change (not scene update)
            Step::RenderWithSnapshot(Duration::from_millis(450)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/tile_resize_entire_component_without_parent_transition",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_tile_resize.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_tile_resize.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/change_order_of_3_inputs_with_id",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_3_inputs_3_id_different_order.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_adding_id",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_3_inputs_1_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/add_2_inputs_at_the_end_to_3_tiles_scene",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_5_inputs_no_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/add_input_on_2nd_pos_to_3_tiles_scene",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_4_inputs_1_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/add_input_at_the_end_to_3_tiles_scene",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_no_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_4_inputs_no_id.scene.json"
            )),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/after_end_with_4_inputs_no_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_changing_id",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_3_inputs_3_id_different_component.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_changing_id_and_add_new_component",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_4_inputs_3_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_changing_id_add_margin",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_3_inputs_3_id_different_component_margin.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_changing_id_add_new_component_last_row_center_aligned",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id_center.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_4_inputs_3_id_center.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles_transitions/replace_component_by_changing_id_add_new_component_last_row_left_aligned",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/start_with_3_inputs_all_id_left.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(0)),
            Step::UpdateSceneJson(include_str!(
                "./tiles_transitions/end_with_4_inputs_3_id_left.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1)),
            Step::RenderWithSnapshot(Duration::from_millis(150)),
            Step::RenderWithSnapshot(Duration::from_millis(350)),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
        ],
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        ..Default::default()
    });

    runner.run()
}
