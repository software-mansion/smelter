use compositor_render::{OutputFrameFormat, Resolution};

use super::{input::TestInput, scene_from_json, snapshots_path, test_case::TestCase, TestRunner};

#[test]
fn view_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("view"));
    let default = TestCase {
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    };

    runner.add(TestCase {
        name: "view/overflow_hidden_with_input_stream_children",
        scene_updates: scene_from_json(include_str!(
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
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/overflow_hidden_with_view_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/constant_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_hidden",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_hidden.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_visible",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_visible.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_views_row_with_overflow_fit",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/constant_width_views_row_with_overflow_fit.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_width_views_row",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/dynamic_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_and_constant_width_views_row",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/dynamic_and_constant_width_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/dynamic_and_constant_width_views_row_with_overflow",
        scene_updates: scene_from_json(
            include_str!("../../snapshot_tests/view/dynamic_and_constant_width_views_row_with_overflow.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/constant_width_and_height_views_row",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/constant_width_and_height_views_row.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_with_absolute_positioning_partially_covered_by_sibling",
        scene_updates: scene_from_json(
            include_str!("../../snapshot_tests/view/view_with_absolute_positioning_partially_covered_by_sibling.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_with_absolute_positioning_render_over_siblings",
        scene_updates: scene_from_json(
            include_str!("../../snapshot_tests/view/view_with_absolute_positioning_render_over_siblings.scene.json"),
        ),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/root_view_with_background_color",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/root_view_with_background_color.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_width",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_width.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/box_shadow",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/box_shadow_sibling",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/box_shadow_sibling.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        only: true,
        allowed_error: 0.0,
        name: "view/border_radius_border_box_shadow",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow.scene.json"
        )),
        output_format: OutputFrameFormat::RgbaWgpuTexture,
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_overflow_hidden",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_overflow_hidden.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_overflow_fit",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_overflow_fit.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_box_shadow_rescaler_input_stream",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_box_shadow_rescaler_input_stream.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius_aligned",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius_aligned.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_border_width_radius_multi_child",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/nested_border_width_radius_multi_child.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        // it is supposed to be cut off because of the rescaler that wraps it
        name: "view/border_radius_border_box_shadow_rescaled",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow_rescaled.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/root_border_radius_border_box_shadow",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/root_border_radius_border_box_shadow.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/border_radius_border_box_shadow_rescaled_and_hidden_by_parent",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/border_radius_border_box_shadow_rescaled_and_hidden_by_parent.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/unsized_view_padding_static_children",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/unsized_view_padding_static_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_padding_multiple_children",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/view_padding_multiple_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_padding_static_children",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/nested_padding_static_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/nested_padding_static_children_overflow_visible",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/nested_padding_static_children_overflow_visible.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/padding_absolute_children",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/padding_absolute_children.scene.json"
        )),
        ..default.clone()
    });
    runner.add(TestCase {
        name: "view/view_padding_overflow_children",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/view/view_padding_overflow_children.scene.json"
        )),
        ..default.clone()
    });

    runner.run()
}
