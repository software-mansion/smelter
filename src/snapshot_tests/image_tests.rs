use compositor_render::{
    image::{ImageSource, ImageSpec, ImageType},
    RendererId, RendererSpec,
};

use super::{
    input::TestInput, snapshots_path, test_case::TestCase, test_steps_from_scene,
    test_steps_from_scenes, TestRunner,
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
            image_type: ImageType::Svg { resolution: None },
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
        only: true,
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
        renderers: vec![jpeg.clone()],
        inputs: vec![],
        ..Default::default()
    });

    runner.run()
}
