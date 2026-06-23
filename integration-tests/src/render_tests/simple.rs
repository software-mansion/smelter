use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId,
    scene::{Component, InputStreamComponent, ViewComponent},
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

#[allow(dead_code)]
pub const TESTS: &[RenderTest] = &[SIMPLE_INPUT_PASS_THROUGH];

#[render_test(description = "Single input stream rendered without any transformations.")]
fn simple_input_pass_through() -> Result<()> {
    let mut runner =
        TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![TestInput::new(1)]);
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::InputStream(InputStreamComponent {
            id: None,
            input_id: InputId("input_1".into()),
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
