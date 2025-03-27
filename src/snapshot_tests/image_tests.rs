use compositor_render::{
    image::{ImageSource, ImageSpec, ImageType},
    RendererId, RendererSpec,
};

use super::{
    input::TestInput, scene_from_json, scenes_from_json, snapshots_path, test_case::TestCase,
    TestRunner,
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
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/image/jpeg_as_root.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/image/jpeg_in_view.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/jpeg_in_view_overflow_fit",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/image/jpeg_in_view_overflow_fit.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![TestInput::new(1)],
        ..Default::default()
    });
    runner.add(TestCase {
        // Test if removing image from scene works
        name: "image/remove_jpeg_as_root",
        scene_updates: scenes_from_json(&[
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
        scene_updates: scenes_from_json(&[
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
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/image/svg_as_root.scene.json"
        )),
        renderers: vec![svg.clone()],
        inputs: vec![],
        ..Default::default()
    });
    runner.add(TestCase {
        name: "image/svg_in_view",
        scene_updates: scene_from_json(include_str!(
            "../../snapshot_tests/image/svg_in_view.scene.json"
        )),
        renderers: vec![jpeg.clone()],
        inputs: vec![],
        ..Default::default()
    });

    runner.run()
}
