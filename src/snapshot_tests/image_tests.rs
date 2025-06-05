use std::time::Duration;

use compositor_render::{
    image::{ImageSource, ImageSpec, ImageType},
    RendererId, RendererSpec,
};

use super::{
    input::TestInput, snapshots_path, test_case::TestCase, test_steps_from_scene,
    test_steps_from_scenes, Step, TestRunner,
};

#[test]
fn image_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("image"));

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
                path: format!(
                    "{}/integration_tests/assets/image.svg",
                    env!("CARGO_MANIFEST_DIR")
                ),
            },
            image_type: ImageType::Svg,
        }),
    );
    let gif1 = (
        RendererId("image_gif1".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: format!(
                    "{}/snapshot_tests/snapshots/demo_assets/donate.gif",
                    env!("CARGO_MANIFEST_DIR")
                ),
            },
            image_type: ImageType::Gif,
        }),
    );
    let gif2 = (
        RendererId("image_gif2".into()),
        RendererSpec::Image(ImageSpec {
            src: ImageSource::LocalPath {
                path: format!(
                    "{}/snapshot_tests/snapshots/assets/progress-bar.gif",
                    env!("CARGO_MANIFEST_DIR")
                ),
            },
            image_type: ImageType::Gif,
        }),
    );

    runner.add(TestCase {
        name: "image/jpeg_as_root",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/image/jpeg_as_root.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/image/jpeg_in_view.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view_overflow_fit",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/image/jpeg_in_view_overflow_fit.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing image from scene works
        name: "image/remove_jpeg_as_root",
        steps: test_steps_from_scenes(&[
            include_str!("../../snapshot_tests/image/jpeg_as_root.scene.json"),
            include_str!("../../snapshot_tests/view/empty_view.scene.json"),
        ]),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing image from scene works
        name: "image/remove_jpeg_in_view",
        steps: test_steps_from_scenes(&[
            include_str!("../../snapshot_tests/image/jpeg_in_view.scene.json"),
            include_str!("../../snapshot_tests/view/empty_view.scene.json"),
        ]),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });

    runner.add(TestCase {
        name: "image/svg_as_root",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/image/svg_as_root.scene.json"
        )),
        renderers: vec![svg.clone()],
        inputs: vec![],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/svg_in_view",
        steps: test_steps_from_scene(include_str!(
            "../../snapshot_tests/image/svg_in_view.scene.json"
        )),
        renderers: vec![svg.clone()],
        inputs: vec![],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/gif_progress_between_updates",
        steps: vec![
            Step::UpdateSceneJson(include_str!(
                "../../snapshot_tests/image/gif_as_root_variant1.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(500)),
            // Update should not reset gif progress
            Step::UpdateSceneJson(include_str!(
                "../../snapshot_tests/image/gif_as_root_variant1.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1000)),
            // Image params changed, the progress should be restarted
            Step::UpdateSceneJson(include_str!(
                "../../snapshot_tests/image/gif_as_root_variant2.scene.json"
            )),
            Step::RenderWithSnapshot(Duration::from_millis(1001)),
        ],
        renderers: vec![gif1, gif2],
        inputs: vec![],
        ..Default::default()
    });

    runner.run()
}
