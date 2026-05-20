use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;

use crate::render_tests::{RenderTest, harness::test_case::TestRunner};

pub const TESTS: &[RenderTest] = &[
    ALIGN_CENTER,
    ALIGN_RIGHT,
    BOLD_TEXT,
    DIMENSIONS_FITTED_COLUMN_WITH_LONG_TEXT,
    DIMENSIONS_FITTED_COLUMN_WITH_SHORT_TEXT,
    DIMENSIONS_FITTED,
    DIMENSIONS_FIXED,
    DIMENSIONS_FIXED_WITH_OVERFLOW,
    RED_TEXT_ON_BLUE_BACKGROUND,
    WRAP_GLYPH,
    WRAP_NONE,
    WRAP_WORD,
    REMOVE_TEXT_IN_VIEW,
    REMOVE_TEXT_AS_ROOT,
    TEXT_AS_ROOT,
];

#[render_test(description = "")]
fn align_center() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/align_center.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/align_right.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn bold_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/bold_text.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted_column_with_long_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./text/dimensions_fitted_column_with_long_text.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted_column_with_short_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./text/dimensions_fitted_column_with_short_text.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/dimensions_fitted.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fixed() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/dimensions_fixed.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fixed_with_overflow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./text/dimensions_fixed_with_overflow.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn red_text_on_blue_background() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!(
        "./text/red_text_on_blue_background.scene.json"
    ));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_glyph() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/wrap_glyph.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_none() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/wrap_none.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_word() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/wrap_word.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_text_in_view() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/align_center.scene.json"));
    runner.update_scene_json(include_str!("./view/empty_view.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_text_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/root_text.scene.json"));
    runner.update_scene_json(include_str!("./view/empty_view.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn text_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene_json(include_str!("./text/root_text.scene.json"));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
