use crate::paths::render_snapshots_dir_path;

use super::{TestRunner, input::TestInput, test_case::TestCase, test_steps_from_scene};

#[test]
fn simple_tests() {
    let mut runner = TestRunner::new(render_snapshots_dir_path().join("simple"));

    runner.add(TestCase {
        name: "simple/simple_input_pass_through",
        inputs: vec![TestInput::new(1)],
        steps: test_steps_from_scene(include_str!(
            "./simple/simple_input_pass_through.scene.json"
        )),
        ..Default::default()
    });

    runner.run()
}
