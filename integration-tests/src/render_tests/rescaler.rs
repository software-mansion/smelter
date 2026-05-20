use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{RenderingMode, Resolution};

use crate::render_tests::{
    RenderTest,
    harness::{DEFAULT_RESOLUTION, input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    FIT_VIEW_WITH_KNOWN_HEIGHT,
    FIT_VIEW_WITH_KNOWN_WIDTH,
    FIT_VIEW_WITH_UNKNOWN_WIDTH_AND_HEIGHT,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_TOP_LEFT,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FILL_INPUT_STREAM_LOWER_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FILL_INPUT_STREAM_LOWER_ASPECT_RATIO,
    FILL_INPUT_STREAM_HIGHER_ASPECT_RATIO,
    FILL_INPUT_STREAM_INVERTED_ASPECT_RATIO,
    FILL_INPUT_STREAM_MATCHING_ASPECT_RATIO,
    FIT_INPUT_STREAM_LOWER_ASPECT_RATIO,
    FIT_INPUT_STREAM_HIGHER_ASPECT_RATIO,
    FIT_INPUT_STREAM_HIGHER_ASPECT_RATIO_SMALL_RESOLUTION,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_TOP_LEFT,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FIT_INPUT_STREAM_LOWER_ASPECT_RATIO_ALIGN_BOTTOM_RIGHT,
    FIT_INPUT_STREAM_INVERTED_ASPECT_RATIO,
    FIT_INPUT_STREAM_MATCHING_ASPECT_RATIO,
    BORDER_RADIUS,
    BORDER_WIDTH,
    BOX_SHADOW,
    BORDER_RADIUS_BORDER_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW,
    BORDER_RADIUS_BOX_SHADOW_FIT_INPUT_STREAM,
    BORDER_RADIUS_BOX_SHADOW_FILL_INPUT_STREAM,
    NESTED_BORDER_WIDTH_RADIUS,
    NESTED_BORDER_WIDTH_RADIUS_ALIGNED,
    BORDER_RADIUS_BORDER_BOX_SHADOW_RESCALED,
    SCALING_FILTER_BILINEAR,
    SCALING_FILTER_LANCZOS3,
];

#[render_test(description = "")]
fn fit_view_with_known_height() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_view_with_known_height.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_view_with_known_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_view_with_known_width.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_view_with_unknown_width_and_height() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_view_with_unknown_width_and_height.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio_align_top_left() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fill_input_stream_align_top_left.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fill_input_stream_align_bottom_right.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_lower_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fill_input_stream_align_bottom_right.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_lower_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fill_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_higher_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height + 100,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fill_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_inverted_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fill_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fill_input_stream_matching_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./rescaler/fill_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_lower_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fit_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_higher_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height + 100,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fit_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_higher_aspect_ratio_small_resolution() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width / 10,
                height: (DEFAULT_RESOLUTION.height + 100) / 10,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fit_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio_align_top_left() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_input_stream_align_top_left.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_input_stream_align_bottom_right.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_lower_aspect_ratio_align_bottom_right() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: DEFAULT_RESOLUTION.width,
                height: DEFAULT_RESOLUTION.height - 100,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/fit_input_stream_align_bottom_right.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_inverted_aspect_ratio() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new_with_resolution(
            1,
            Resolution {
                width: 360,
                height: 640,
            },
        )]);
    runner.update_scene_json(include_str!("./rescaler/fit_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn fit_input_stream_matching_aspect_ratio() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./rescaler/fit_input_stream.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./rescaler/border_radius.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_width() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./rescaler/border_width.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!("./rescaler/box_shadow.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/border_radius_border_box_shadow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/border_radius_box_shadow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_fit_input_stream() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/border_radius_box_shadow_fit_input_stream.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_box_shadow_fill_input_stream() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/border_radius_box_shadow_fill_input_stream.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/nested_border_width_radius.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn nested_border_width_radius_aligned() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/nested_border_width_radius_aligned.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn border_radius_border_box_shadow_rescaled() -> Result<()> {
    // it is supposed to be cut off because of the rescaler that wraps it
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene_json(include_str!(
        "./rescaler/border_radius_border_box_shadow_rescaled.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn scaling_filter_bilinear() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_rendering_mode(RenderingMode::CpuOptimized)
        .with_resolution(Resolution {
            width: 1920,
            height: 1080,
        })
        .with_inputs(vec![TestInput::new_multiscale_grid(
            1,
            Resolution {
                width: 5760,
                height: 3240,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/scaling_filter_bilinear.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn scaling_filter_lanczos3() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_rendering_mode(RenderingMode::GpuOptimized)
        .with_resolution(Resolution {
            width: 1920,
            height: 1080,
        })
        .with_inputs(vec![TestInput::new_multiscale_grid(
            1,
            Resolution {
                width: 5760,
                height: 3240,
            },
        )]);
    runner.update_scene_json(include_str!(
        "./rescaler/scaling_filter_lanczos3.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
