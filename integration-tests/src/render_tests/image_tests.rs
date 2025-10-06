use std::time::Duration;

use smelter_render::{
    RendererId, RendererSpec,
    image::{ImageSource, ImageSpec, ImageType},
};

use crate::paths::{integration_tests_root, render_snapshots_dir_path, submodule_root_path};

use super::{
    Step, TestRunner, input::TestInput, test_case::TestCase, test_steps_from_scene,
    test_steps_from_scenes,
};

#[test]
fn image_tests() {
    let mut runner = TestRunner::new(render_snapshots_dir_path().join("image"));

    let jpeg = (
        RendererId("image_jpeg".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::Url {
                url: "https://www.rust-lang.org/static/images/rust-social.jpg".to_string(),
            },
            image_type: ImageType::Jpeg,
        }),
    );
    let svg = (
        RendererId("image_svg".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: integration_tests_root()
                    .join("assets/image.svg")
                    .to_string_lossy()
                    .to_string(),
            },
            image_type: ImageType::Svg,
        }),
    );
    let gif1 = (
        RendererId("image_gif1".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: submodule_root_path()
                    .join("demo_assets/donate.gif")
                    .to_string_lossy()
                    .to_string(),
            },
            image_type: ImageType::Gif,
        }),
    );
    let gif2 = (
        RendererId("image_gif2".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: submodule_root_path()
                    .join("assets/progress-bar.gif")
                    .to_string_lossy()
                    .to_string(),
            },
            image_type: ImageType::Gif,
        }),
    );

    runner.add(TestCase {
        name: "image/jpeg_as_root",
        steps: test_steps_from_scene(include_str!("./image/jpeg_as_root.scene.json")),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view",
        steps: test_steps_from_scene(include_str!("./image/jpeg_in_view.scene.json")),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view_overflow_fit",
        steps: test_steps_from_scene(include_str!("./image/jpeg_in_view_overflow_fit.scene.json")),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing image from scene works
        name: "image/remove_jpeg_as_root",
        steps: test_steps_from_scenes(&[
            include_str!("./image/jpeg_as_root.scene.json"),
            include_str!("./view/empty_view.scene.json"),
        ]),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing image from scene works
        name: "image/remove_jpeg_in_view",
        steps: test_steps_from_scenes(&[
            include_str!("./image/jpeg_in_view.scene.json"),
            include_str!("./view/empty_view.scene.json"),
        ]),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });

    runner.add(TestCase {
        name: "image/svg_as_root",
        steps: test_steps_from_scene(include_str!("./image/svg_as_root.scene.json")),
        renderers: vec![svg.clone()],
        inputs: vec![],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/svg_in_view",
        steps: test_steps_from_scene(include_str!("./image/svg_in_view.scene.json")),
        renderers: vec![svg.clone()],
        inputs: vec![],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/gif_progress_between_updates",
        steps: vec![
            Step::UpdateSceneJson(include_str!("./image/gif_as_root_variant1.scene.json")),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
            // Update should not reset gif progress
            Step::UpdateSceneJson(include_str!("./image/gif_as_root_variant1.scene.json")),
            Step::RenderWithSnapshot(Duration::from_millis(1000)),
            // Image params changed, the progress should be restarted
            Step::UpdateSceneJson(include_str!("./image/gif_as_root_variant2.scene.json")),
            Step::RenderWithSnapshot(Duration::from_millis(1001)),
        ],
        renderers: vec![gif1, gif2],
        inputs: vec![],
        ..Default::default()
    });

    runner.run()
}
