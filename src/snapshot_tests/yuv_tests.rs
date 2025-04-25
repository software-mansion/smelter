use core::panic;
use std::{sync::Arc, time::Duration};

use compositor_render::{
    scene::{
        BorderRadius, Component, Overflow, Position, RGBAColor, ShaderComponent, Size,
        ViewChildrenDirection, ViewComponent,
    },
    shader::ShaderSpec,
    OutputFrameFormat, RendererId, RendererSpec, Resolution,
};

use super::{test_case::TestCase, Step};

fn run_case(test_case: TestCase, expected: &[u8]) {
    let snapshots = test_case.generate_snapshots();
    let failed = snapshots[0]
        .data
        .iter()
        .zip(expected)
        .any(|(actual, expected)| u8::abs_diff(*actual, *expected) > 2);
    if failed {
        panic!("Sample mismatched {:?}", snapshots[0].data)
    }
}

/// Test how yuv output is generated for smooth color change
#[test]
fn yuv_test_gradient() {
    let shader_id = RendererId(Arc::from("example_shader"));
    let width = 8;
    let height = 2;

    let yuv_case = TestCase {
        renderers: vec![(
            shader_id.clone(),
            RendererSpec::Shader(ShaderSpec {
                source: include_str!("./yuv_tests/gradient.wgsl").into(),
            }),
        )],
        resolution: Resolution { width, height },
        steps: vec![
            Step::UpdateScene(Component::Shader(ShaderComponent {
                id: None,
                children: vec![],
                shader_id: shader_id.clone(),
                shader_param: None,
                size: Size {
                    width: width as f32,
                    height: height as f32,
                },
            })),
            Step::RenderWithSnapshot(Duration::ZERO),
        ],
        ..Default::default()
    };
    let rgb_case = TestCase {
        output_format: OutputFrameFormat::RgbaWgpuTexture,
        ..yuv_case.clone()
    };

    #[rustfmt::skip]
    run_case(
        yuv_case,
        &[
            88, 0, 0, 255, 103, 7, 7, 255, 159, 0, 0, 255, 167, 4, 3, 255, 204, 0, 0, 255, 210, 2, 2, 255, 238, 0, 0, 255, 242, 2, 1, 255, 
            88, 0, 0, 255, 103, 7, 7, 255, 159, 0, 0, 255, 167, 4, 3, 255, 204, 0, 0, 255, 210, 2, 2, 255, 238, 0, 0, 255, 242, 2, 1, 255
        ]
    );
    #[rustfmt::skip]
    run_case(rgb_case,
        &[
            71, 0, 0, 255, 120, 0, 0, 255, 152, 0, 0, 255, 177, 0, 0, 255, 198, 0, 0, 255, 216, 0, 0, 255, 233, 0, 0, 255, 248, 0, 0, 255,
            71, 0, 0, 255, 120, 0, 0, 255, 152, 0, 0, 255, 177, 0, 0, 255, 198, 0, 0, 255, 216, 0, 0, 255, 233, 0, 0, 255, 248, 0, 0, 255,
        ],
    );
}

/// Test how yuv output is generated for unified color
#[test]
fn yuv_test_uniform_color() {
    let width = 8;
    let height = 2;

    let yuv_case = TestCase {
        resolution: Resolution { width, height },
        steps: vec![
            Step::UpdateScene(Component::View(ViewComponent {
                id: None,
                children: vec![],
                direction: ViewChildrenDirection::Row,
                position: Position::Static {
                    width: None,
                    height: None,
                },
                transition: None,
                overflow: Overflow::Hidden,
                background_color: RGBAColor(50, 0, 0, 255),
                border_radius: BorderRadius::ZERO,
                border_width: 0.0,
                border_color: RGBAColor(0, 0, 0, 0),
                box_shadow: vec![],
                padding: Default::default(),
            })),
            Step::RenderWithSnapshot(Duration::ZERO),
        ],
        ..Default::default()
    };
    let rgb_case = TestCase {
        output_format: OutputFrameFormat::RgbaWgpuTexture,
        ..yuv_case.clone()
    };

    #[rustfmt::skip]
    run_case(
        yuv_case,
        &[
            50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255,
            50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255
        ],
    );
    #[rustfmt::skip]
    run_case(rgb_case,
        &[
            50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255,
            50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255, 50, 0, 0, 255
        ],
    );
}
