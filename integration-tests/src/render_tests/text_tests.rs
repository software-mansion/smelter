use crate::paths::render_snapshots_dir_path;

use super::{test_case::TestCase, test_steps_from_scene, test_steps_from_scenes, TestRunner};

#[test]
fn text_tests() {
    let mut runner = TestRunner::new(render_snapshots_dir_path().join("text"));

    runner.add(TestCase {
        name: "text/align_center",
        steps: test_steps_from_scene(include_str!("./text/align_center.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/align_right",
        steps: test_steps_from_scene(include_str!("./text/align_right.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/bold_text",
        steps: test_steps_from_scene(include_str!("./text/bold_text.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/dimensions_fitted_column_with_long_text",
        steps: test_steps_from_scene(include_str!(
            "./text/dimensions_fitted_column_with_long_text.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/dimensions_fitted_column_with_short_text",
        steps: test_steps_from_scene(include_str!(
            "./text/dimensions_fitted_column_with_short_text.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/dimensions_fitted",
        steps: test_steps_from_scene(include_str!("./text/dimensions_fitted.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/dimensions_fixed",
        steps: test_steps_from_scene(include_str!("./text/dimensions_fixed.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/dimensions_fixed_with_overflow",
        steps: test_steps_from_scene(include_str!(
            "./text/dimensions_fixed_with_overflow.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/red_text_on_blue_background",
        steps: test_steps_from_scene(include_str!(
            "./text/red_text_on_blue_background.scene.json"
        )),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/wrap_glyph",
        steps: test_steps_from_scene(include_str!("./text/wrap_glyph.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/wrap_none",
        steps: test_steps_from_scene(include_str!("./text/wrap_none.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/wrap_word",
        steps: test_steps_from_scene(include_str!("./text/wrap_word.scene.json")),
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing text from scene works
        name: "text/remove_text_in_view",
        steps: test_steps_from_scenes(&[
            include_str!("./text/align_center.scene.json"),
            include_str!("./view/empty_view.scene.json"),
        ]),
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing text from scene works
        name: "text/remove_text_as_root",
        steps: test_steps_from_scenes(&[
            include_str!("./text/root_text.scene.json"),
            include_str!("./view/empty_view.scene.json"),
        ]),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "text/text_as_root",
        steps: test_steps_from_scene(include_str!("./text/root_text.scene.json")),
        ..Default::default()
    });

    runner.run()
}
