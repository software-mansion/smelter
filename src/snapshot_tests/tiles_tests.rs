use compositor_render::Resolution;

use super::{
    input::TestInput, snapshots_path, test_case::TestCase, test_steps_from_scene, TestRunner,
};

#[test]
fn tiles_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("tiles"));

    let input1 = TestInput::new(1);
    let input2 = TestInput::new(2);
    let input3 = TestInput::new(3);
    let input4 = TestInput::new(4);
    let input5 = TestInput::new(5);
    let input6 = TestInput::new(6);
    let input7 = TestInput::new(7);
    let input8 = TestInput::new(8);
    let input9 = TestInput::new(9);
    let input10 = TestInput::new(10);
    let input11 = TestInput::new(11);
    let input12 = TestInput::new(12);
    let input13 = TestInput::new(13);
    let input14 = TestInput::new(14);
    let input15 = TestInput::new(15);
    let portrait_resolution = Resolution {
        width: 360,
        height: 640,
    };
    let portrait_input1 = TestInput::new_with_resolution(1, portrait_resolution);
    let portrait_input2 = TestInput::new_with_resolution(2, portrait_resolution);
    let portrait_input3 = TestInput::new_with_resolution(3, portrait_resolution);
    let portrait_input4 = TestInput::new_with_resolution(4, portrait_resolution);
    let portrait_input5 = TestInput::new_with_resolution(5, portrait_resolution);
    let portrait_input6 = TestInput::new_with_resolution(6, portrait_resolution);
    let portrait_input7 = TestInput::new_with_resolution(7, portrait_resolution);
    let portrait_input8 = TestInput::new_with_resolution(8, portrait_resolution);
    let portrait_input9 = TestInput::new_with_resolution(9, portrait_resolution);
    let portrait_input10 = TestInput::new_with_resolution(10, portrait_resolution);
    let portrait_input11 = TestInput::new_with_resolution(11, portrait_resolution);
    let portrait_input12 = TestInput::new_with_resolution(12, portrait_resolution);
    let portrait_input13 = TestInput::new_with_resolution(13, portrait_resolution);
    let portrait_input14 = TestInput::new_with_resolution(14, portrait_resolution);
    let portrait_input15 = TestInput::new_with_resolution(15, portrait_resolution);

    runner.add(TestCase {
        name: "tiles/01_inputs",
        inputs: vec![input1.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/01_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/02_inputs",
        inputs: vec![input1.clone(), input2.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/02_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/04_inputs",
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/04_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/05_inputs",
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/05_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/15_inputs",
        inputs: vec![
            input1.clone(),
            input2.clone(),
            input3.clone(),
            input4.clone(),
            input5.clone(),
            input6.clone(),
            input7.clone(),
            input8.clone(),
            input9.clone(),
            input10.clone(),
            input11.clone(),
            input12.clone(),
            input13.clone(),
            input14.clone(),
            input15.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/15_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/01_portrait_inputs",
        inputs: vec![portrait_input1.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/01_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/02_portrait_inputs",
        inputs: vec![portrait_input1.clone(), portrait_input2.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/02_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/03_portrait_inputs",
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/03_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/05_portrait_inputs",
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
            portrait_input4.clone(),
            portrait_input5.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/05_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/15_portrait_inputs",
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
            portrait_input4.clone(),
            portrait_input5.clone(),
            portrait_input6.clone(),
            portrait_input7.clone(),
            portrait_input8.clone(),
            portrait_input9.clone(),
            portrait_input10.clone(),
            portrait_input11.clone(),
            portrait_input12.clone(),
            portrait_input13.clone(),
            portrait_input14.clone(),
            portrait_input15.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/15_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/01_portrait_inputs_on_portrait_output",
        resolution: portrait_resolution,
        inputs: vec![portrait_input1.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/01_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/03_portrait_inputs_on_portrait_output",
        resolution: portrait_resolution,
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/03_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/03_inputs_on_portrait_output",
        resolution: portrait_resolution,
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/05_portrait_inputs_on_portrait_output",
        resolution: portrait_resolution,
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
            portrait_input4.clone(),
            portrait_input5.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/05_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/15_portrait_inputs_on_portrait_output",
        resolution: portrait_resolution,
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
            portrait_input4.clone(),
            portrait_input5.clone(),
            portrait_input6.clone(),
            portrait_input7.clone(),
            portrait_input8.clone(),
            portrait_input9.clone(),
            portrait_input10.clone(),
            portrait_input11.clone(),
            portrait_input12.clone(),
            portrait_input13.clone(),
            portrait_input14.clone(),
            portrait_input15.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/15_portrait_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/align_center_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/align_center_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/align_top_left_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/align_top_left_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/align_with_margin_and_padding_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/align_with_margin_and_padding_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/margin_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/margin_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/margin_and_padding_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/margin_and_padding_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/padding_with_03_inputs",
        inputs: vec![input1.clone(), input2.clone(), input3.clone()],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/padding_with_03_inputs.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "tiles/video_call_with_labels",
        inputs: vec![
            portrait_input1.clone(),
            portrait_input2.clone(),
            portrait_input3.clone(),
        ],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/tiles/video_call_with_labels.scene.json"
        )),
        ..Default::default()
    });

    runner.run()
}
