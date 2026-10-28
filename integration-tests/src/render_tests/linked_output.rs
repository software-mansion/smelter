use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, RenderingMode, Resolution,
    scene::{Component, InputStreamComponent, RescaleMode, RescalerComponent},
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[
    LINKED_OUTPUT_DOWNSCALE_LANCZOS3,
    LINKED_OUTPUT_DOWNSCALE_BILINEAR,
    LINKED_OUTPUT_DIFFERENT_ASPECT_RATIO,
];

const PRIMARY_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

fn runner(module: &'static str, test_name: &'static str, mode: RenderingMode) -> TestRunner {
    let mut runner = TestRunner::new(module, test_name)
        .with_rendering_mode(mode)
        .with_resolution(PRIMARY_RESOLUTION)
        .with_inputs(vec![TestInput::new_multiscale_grid(1, PRIMARY_RESOLUTION)]);
    runner.update_scene(Component::Rescaler(RescalerComponent {
        mode: RescaleMode::Fit,
        child: Box::new(Component::InputStream(InputStreamComponent {
            id: None,
            input_id: InputId("input_1".into()),
        })),
        ..Default::default()
    }));
    runner
}

#[render_test(description = "")]
fn linked_output_downscale_lanczos3() -> Result<()> {
    let mut runner = runner(MODULE, TEST_NAME, RenderingMode::GpuOptimized);
    runner.link_output(Resolution {
        width: 640,
        height: 360,
    });
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn linked_output_downscale_bilinear() -> Result<()> {
    let mut runner = runner(MODULE, TEST_NAME, RenderingMode::CpuOptimized);
    runner.link_output(Resolution {
        width: 640,
        height: 360,
    });
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "Aspect ratio is not preserved, frame is stretched.")]
fn linked_output_different_aspect_ratio() -> Result<()> {
    let mut runner = runner(MODULE, TEST_NAME, RenderingMode::GpuOptimized);
    runner.link_output(Resolution {
        width: 640,
        height: 480,
    });
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
