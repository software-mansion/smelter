use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::Resolution;

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    OVERFLOW_HIDDEN_WITH_INPUT_STREAM_CHILDREN,
    OVERFLOW_HIDDEN_WITH_VIEW_CHILDREN,
    CONSTANT_WIDTH_VIEWS_ROW,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_HIDDEN,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_VISIBLE,
    CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW_FIT,
    DYNAMIC_WIDTH_VIEWS_ROW,
    DYNAMIC_AND_CONSTANT_WIDTH_VIEWS_ROW,
    DYNAMIC_AND_CONSTANT_WIDTH_VIEWS_ROW_WITH_OVERFLOW,
    CONSTANT_WIDTH_AND_HEIGHT_VIEWS_ROW,
    VIEW_WITH_ABSOLUTE_POSITIONING_PARTIALLY_COVERED_BY_SIBLING,
    VIEW_WITH_ABSOLUTE_POSITIONING_RENDER_OVER_SIBLINGS,
    ROOT_VIEW_WITH_BACKGROUND_COLOR,
    BORDER_RADIUS,
    BORDER_RADIUS_CLIPPING,
    BORDER_RADIUS_CLIPPING_LARGE_BORDER_WIDTH,
    BORDER_WIDTH,
    BOX_SHADOW,
    BOX_SHADOW_SIBLING,
    BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW_OVERFLOW_HIDDEN,
    BORDER_RADIUS_BOX_SHADOW_OVERFLOW_FIT,
    BORDER_RADIUS_BOX_SHADOW_RESCALER_INPUT_STREAM,
    NESTED_BORDER_WIDTH_RADIUS,
    NESTED_BORDER_WIDTH_RADIUS_ALIGNED,
    NESTED_BORDER_WIDTH_RADIUS_MULTI_CHILD,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED,
    ROOT_BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED_AND_HIDDEN_BY_PARENT,
    UNSIZED_VIEW_PADDING_STATIC_CHILDREN,
    VIEW_PADDING_MULTIPLE_CHILDREN,
    NESTED_PADDING_STATIC_CHILDREN,
    NESTED_PADDING_STATIC_CHILDREN_OVERFLOW_VISIBLE,
    PADDING_ABSOLUTE_CHILDREN,
    VIEW_PADDING_OVERFLOW_CHILDREN,
];

#[render_test(description = "")]
fn overflow_hidden_with_input_stream_children() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 180,
                height: 200,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./view/overflow_hidden_with_input_stream_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn overflow_hidden_with_view_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/overflow_hidden_with_view_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/constant_width_views_row.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_hidden() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/constant_width_views_row_with_overflow_hidden.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_visible() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/constant_width_views_row_with_overflow_visible.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_views_row_with_overflow_fit() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/constant_width_views_row_with_overflow_fit.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/dynamic_width_views_row.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_and_constant_width_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/dynamic_and_constant_width_views_row.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dynamic_and_constant_width_views_row_with_overflow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/dynamic_and_constant_width_views_row_with_overflow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn constant_width_and_height_views_row() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/constant_width_and_height_views_row.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_with_absolute_positioning_partially_covered_by_sibling() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/view_with_absolute_positioning_partially_covered_by_sibling.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_with_absolute_positioning_render_over_siblings() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/view_with_absolute_positioning_render_over_siblings.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn root_view_with_background_color() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/root_view_with_background_color.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/border_radius.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_clipping() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/border_radius_clipping.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_clipping_large_border_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_clipping_large_border_width.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/border_width.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/box_shadow.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow_sibling() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/box_shadow_sibling.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_border_box_shadow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/border_radius_box_shadow.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_overflow_hidden() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_box_shadow_overflow_hidden.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_overflow_fit() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_box_shadow_overflow_fit.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_rescaler_input_stream() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_box_shadow_rescaler_input_stream.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/nested_border_width_radius.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_aligned() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/nested_border_width_radius_aligned.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_multi_child() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/nested_border_width_radius_multi_child.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_border_box_shadow_rescaled.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn root_border_radius_border_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/root_border_radius_border_box_shadow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled_and_hidden_by_parent() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/border_radius_border_box_shadow_rescaled_and_hidden_by_parent.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn unsized_view_padding_static_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/unsized_view_padding_static_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_padding_multiple_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/view_padding_multiple_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_padding_static_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/nested_padding_static_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_padding_static_children_overflow_visible() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/nested_padding_static_children_overflow_visible.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn padding_absolute_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./view/padding_absolute_children.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn view_padding_overflow_children() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./view/view_padding_overflow_children.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
