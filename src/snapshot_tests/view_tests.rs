use smelter_render::Resolution;

use super::{
    input::TestInput, snapshots_path, test_case::TestCase, test_steps_from_scene, TestRunner,
};

#[test]
fn view_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("view"));
    let default = TestCase {
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    };

    runner.add(TestCase {
        name: "view/overflow_hidden_with_input_stream_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/overflow_hidden_with_input_stream_children.scene.json"
        )),
        inputs: vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 180,
                height: 200,
            },
        )],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "view/overflow_hidden_with_view_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/overflow_hidden_with_view_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/constant_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_hidden",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_hidden.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_visible",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_visible.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_fit",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_fit.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_width_views_row",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/dynamic_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_and_constant_width_views_row",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/dynamic_and_constant_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_and_constant_width_views_row_with_overflow",
        steps: test_steps_from_scene(
            include_str!("../../snapshot_tests/view/dynamic_and_constant_width_views_row_with_overflow.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_and_height_views_row",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/constant_width_and_height_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_with_absolute_positioning_partially_covered_by_sibling",
        steps: test_steps_from_scene(
            include_str!("../../snapshot_tests/view/view_with_absolute_positioning_partially_covered_by_sibling.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_with_absolute_positioning_render_over_siblings",
        steps: test_steps_from_scene(
            include_str!("../../snapshot_tests/view/view_with_absolute_positioning_render_over_siblings.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/root_view_with_background_color",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/root_view_with_background_color.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_clipping",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_clipping.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_clipping_large_border_width",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_clipping_large_border_width.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_width",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_width.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/box_shadow",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/box_shadow_sibling",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/box_shadow_sibling.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_border_box_shadow",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_overflow_hidden",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_overflow_hidden.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_overflow_fit",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_overflow_fit.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_rescaler_input_stream",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_rescaler_input_stream.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius_aligned",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius_aligned.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius_multi_child",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius_multi_child.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        // it is supposed to be cut off because of the rescaler that wraps it
        name: "view/border_radius_border_box_shadow_rescaled",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow_rescaled.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/root_border_radius_border_box_shadow",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/root_border_radius_border_box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_border_box_shadow_rescaled_and_hidden_by_parent",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow_rescaled_and_hidden_by_parent.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/unsized_view_padding_static_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/unsized_view_padding_static_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_padding_multiple_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/view_padding_multiple_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_padding_static_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/nested_padding_static_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_padding_static_children_overflow_visible",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/nested_padding_static_children_overflow_visible.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/padding_absolute_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/padding_absolute_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_padding_overflow_children",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/view/view_padding_overflow_children.scene.json"
        )),
        ..default.clone()
    });

    runner.run()
}
