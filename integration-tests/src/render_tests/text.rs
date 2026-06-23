use std::{sync::Arc, time::Duration};

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::scene::{
    Component, HorizontalAlign, Overflow, RGBAColor, TextComponent, TextDimensions,
    TextWeight, TextWrap, ViewComponent,
};

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

const EXAMPLE_TEXT: &str = "Example text";
const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Vestibulum id eros non eros dictum scelerisque. Sed vehicula magna et metus fringilla, nec placerat felis elementum. Nullam tincidunt dui id purus egestas, et pulvinar est facilisis.";

fn view_with(child: TextComponent) -> Component {
    Component::View(ViewComponent {
        children: vec![Component::Text(child)],
        overflow: Overflow::Fit,
        ..Default::default()
    })
}

#[render_test(description = "")]
fn align_center() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        align: HorizontalAlign::Center,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_right() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        align: HorizontalAlign::Right,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn bold_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        align: HorizontalAlign::Right,
        weight: TextWeight::Bold,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted_column_with_long_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(
            "Example long text that should be longer that underlaying texture.",
        ),
        font_size: 30.0,
        line_height: 30.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::FittedColumn { width: 300.0, max_height: 4320.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted_column_with_short_text() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 30.0,
        line_height: 30.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::FittedColumn { width: 300.0, max_height: 4320.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fitted() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        background_color: RGBAColor(255, 0, 0, 255),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fixed() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn dimensions_fixed_with_overflow() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 120.0,
        line_height: 120.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::Fixed { width: 640.0, height: 80.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn red_text_on_blue_background() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 50.0,
        line_height: 50.0,
        font_family: Arc::from("Inter"),
        wrap: TextWrap::Word,
        color: RGBAColor(255, 0, 0, 255),
        background_color: RGBAColor(0, 0, 255, 255),
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 500.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_glyph() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(LOREM_IPSUM),
        font_size: 50.0,
        line_height: 50.0,
        font_family: Arc::from("Inter"),
        wrap: TextWrap::Glyph,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 500.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_none() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(LOREM_IPSUM),
        font_size: 50.0,
        line_height: 50.0,
        font_family: Arc::from("Inter"),
        wrap: TextWrap::None,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 500.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn wrap_word() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(LOREM_IPSUM),
        font_size: 50.0,
        line_height: 50.0,
        font_family: Arc::from("Inter"),
        wrap: TextWrap::Word,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 500.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_text_in_view() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(view_with(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        align: HorizontalAlign::Center,
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.update_scene(Component::View(ViewComponent::default()));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_text_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(Component::Text(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.update_scene(Component::View(ViewComponent::default()));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn text_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME);
    runner.update_scene(Component::Text(TextComponent {
        text: Arc::from(EXAMPLE_TEXT),
        font_size: 100.0,
        line_height: 100.0,
        font_family: Arc::from("Inter"),
        dimensions: TextDimensions::Fixed { width: 1000.0, height: 200.0 },
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
