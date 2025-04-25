use super::{
    input::TestInput, snapshots_path, test_case::TestCase, test_steps_from_scene, TestRunner,
};

#[test]
fn simple_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("simple"));

    runner.add(TestCase {
        name: "simple/simple_input_pass_through",
        inputs: vec![TestInput::new(1)],
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/simple/simple_input_pass_through.scene.json"
        )),
        ..Default::default()
    });

    runner.run()
}
