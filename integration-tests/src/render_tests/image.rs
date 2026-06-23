use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    RendererId, RendererSpec,
    image::{ImageSource, ImageSpec, ImageType},
    scene::{Component, ComponentId, ImageComponent, Overflow, ViewComponent},
};

use crate::paths::{integration_tests_root, submodule_root_path};
use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    JPEG_AS_ROOT,
    JPEG_IN_VIEW,
    JPEG_IN_VIEW_OVERFLOW_FIT,
    REMOVE_JPEG_AS_ROOT,
    REMOVE_JPEG_IN_VIEW,
    SVG_AS_ROOT,
    SVG_IN_VIEW,
    GIF_PROGRESS_BETWEEN_UPDATES,
];

const JPEG_ID: &str = "image_jpeg";
const SVG_ID: &str = "image_svg";
const GIF1_ID: &str = "image_gif1";
const GIF2_ID: &str = "image_gif2";

fn jpeg_renderer() -> (RendererId, RendererSpec) {
    (
        RendererId(JPEG_ID.into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::Url {
                url: "https://www.rust-lang.org/static/images/rust-social.jpg".into(),
            },
            image_type: ImageType::Jpeg,
        }),
    )
}

fn svg_renderer() -> (RendererId, RendererSpec) {
    (
        RendererId(SVG_ID.into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: integration_tests_root().join("assets/image.svg").into(),
            },
            image_type: ImageType::Svg,
        }),
    )
}

fn gif1_renderer() -> (RendererId, RendererSpec) {
    (
        RendererId(GIF1_ID.into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: submodule_root_path().join("demo_assets/donate.gif").into(),
            },
            image_type: ImageType::Gif,
        }),
    )
}

fn gif2_renderer() -> (RendererId, RendererSpec) {
    (
        RendererId(GIF2_ID.into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: submodule_root_path().join("assets/progress-bar.gif").into(),
            },
            image_type: ImageType::Gif,
        }),
    )
}

#[render_test(description = "")]
fn jpeg_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![jpeg_renderer()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::Image(ImageComponent {
        image_id: RendererId(JPEG_ID.into()),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn jpeg_in_view() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![jpeg_renderer()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::Image(ImageComponent {
            image_id: RendererId(JPEG_ID.into()),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn jpeg_in_view_overflow_fit() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![jpeg_renderer()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::Image(ImageComponent {
            image_id: RendererId(JPEG_ID.into()),
            ..Default::default()
        })],
        overflow: Overflow::Fit,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_jpeg_as_root() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![jpeg_renderer()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::Image(ImageComponent {
        image_id: RendererId(JPEG_ID.into()),
        ..Default::default()
    }));
    runner.update_scene(Component::View(ViewComponent::default()));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn remove_jpeg_in_view() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![jpeg_renderer()])
        .with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::Image(ImageComponent {
            image_id: RendererId(JPEG_ID.into()),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.update_scene(Component::View(ViewComponent::default()));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn svg_as_root() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_renderers(vec![svg_renderer()]);
    runner.update_scene(Component::Image(ImageComponent {
        image_id: RendererId(SVG_ID.into()),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn svg_in_view() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_renderers(vec![svg_renderer()]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::Image(ImageComponent {
            image_id: RendererId(SVG_ID.into()),
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn gif_progress_between_updates() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_renderers(vec![gif1_renderer(), gif2_renderer()]);
    runner.update_scene(Component::Image(ImageComponent {
        id: Some(ComponentId("gif".into())),
        image_id: RendererId(GIF1_ID.into()),
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(500));
    // Update should not reset gif progress
    runner.update_scene(Component::Image(ImageComponent {
        id: Some(ComponentId("gif".into())),
        image_id: RendererId(GIF1_ID.into()),
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1000));
    // Image params changed, the progress should be restarted
    runner.update_scene(Component::Image(ImageComponent {
        id: Some(ComponentId("gif".into())),
        image_id: RendererId(GIF2_ID.into()),
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1001));
    runner.finish()
}
