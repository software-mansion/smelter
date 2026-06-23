use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId, Resolution,
    scene::{
        AbsolutePosition, Component, HorizontalAlign, HorizontalPosition,
        InputStreamComponent, Position, RGBAColor, RescalerComponent, TextComponent,
        TextDimensions, TilesComponent, VerticalAlign, VerticalPosition, ViewComponent,
    },
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    TILES_01_INPUTS,
    TILES_02_INPUTS,
    TILES_03_INPUTS,
    TILES_04_INPUTS,
    TILES_05_INPUTS,
    TILES_15_INPUTS,
    TILES_01_PORTRAIT_INPUTS,
    TILES_02_PORTRAIT_INPUTS,
    TILES_03_PORTRAIT_INPUTS,
    TILES_05_PORTRAIT_INPUTS,
    TILES_15_PORTRAIT_INPUTS,
    TILES_01_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_03_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_03_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_05_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    TILES_15_PORTRAIT_INPUTS_ON_PORTRAIT_OUTPUT,
    ALIGN_CENTER_WITH_03_INPUTS,
    ALIGN_TOP_LEFT_WITH_03_INPUTS,
    ALIGN_WITH_MARGIN_AND_PADDING_WITH_03_INPUTS,
    MARGIN_WITH_03_INPUTS,
    MARGIN_AND_PADDING_WITH_03_INPUTS,
    PADDING_WITH_03_INPUTS,
    VIDEO_CALL_WITH_LABELS,
];

const PORTRAIT_RESOLUTION: Resolution = Resolution { width: 360, height: 640 };

const BG: RGBAColor = RGBAColor(0x33, 0x33, 0x33, 255);

fn inputs(count: usize) -> Vec<TestInput> {
    (1..=count).map(TestInput::new).collect()
}

fn portrait_inputs(count: usize) -> Vec<TestInput> {
    (1..=count).map(|i| TestInput::new_with_resolution(i, PORTRAIT_RESOLUTION)).collect()
}

fn input_streams(count: usize) -> Vec<Component> {
    (1..=count)
        .map(|i| {
            Component::InputStream(InputStreamComponent {
                id: None,
                input_id: InputId(format!("input_{i}").into()),
            })
        })
        .collect()
}

#[render_test(description = "")]
fn tiles_01_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(1));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(1),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_02_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(2));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(2),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_04_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(4));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(4),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(5));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(5),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(15));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(15),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_01_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(1));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(1),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_02_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(2));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(2),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(5));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(5),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_portrait_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(15));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(15),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_01_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(portrait_inputs(1));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(1),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(portrait_inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_03_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_05_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(portrait_inputs(5));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(5),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn tiles_15_portrait_inputs_on_portrait_output() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME)
        .with_resolution(PORTRAIT_RESOLUTION)
        .with_inputs(portrait_inputs(15));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(15),
        background_color: BG,
        tile_aspect_ratio: (1, 2),
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_center_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        vertical_align: VerticalAlign::Center,
        horizontal_align: HorizontalAlign::Center,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_top_left_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        vertical_align: VerticalAlign::Top,
        horizontal_align: HorizontalAlign::Left,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn align_with_margin_and_padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        vertical_align: VerticalAlign::Top,
        horizontal_align: HorizontalAlign::Left,
        margin: 20.0,
        padding: 20.0,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn margin_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        margin: 50.0,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn margin_and_padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        margin: 20.0,
        padding: 20.0,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn padding_with_03_inputs() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(inputs(3));
    runner.update_scene(Component::Tiles(TilesComponent {
        children: input_streams(3),
        background_color: BG,
        padding: 50.0,
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}

#[render_test(description = "")]
fn video_call_with_labels() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(portrait_inputs(3));
    let labeled_tile = |i: usize| {
        Component::View(ViewComponent {
            background_color: RGBAColor(0x55, 0x55, 0x55, 255),
            children: vec![
                Component::Rescaler(RescalerComponent {
                    child: Box::new(Component::InputStream(InputStreamComponent {
                        id: None,
                        input_id: InputId(format!("input_{i}").into()),
                    })),
                    ..Default::default()
                }),
                Component::View(ViewComponent {
                    position: Position::Absolute(AbsolutePosition {
                        width: None,
                        height: Some(40.0),
                        position_horizontal: HorizontalPosition::LeftOffset(0.0),
                        position_vertical: VerticalPosition::BottomOffset(0.0),
                        rotation_degrees: 0.0,
                    }),
                    children: vec![
                        Component::View(ViewComponent::default()),
                        Component::Text(TextComponent {
                            text: format!("InputStream {i}").into(),
                            font_size: 25.0,
                            line_height: 25.0,
                            align: HorizontalAlign::Center,
                            color: RGBAColor(255, 255, 255, 255),
                            background_color: RGBAColor(255, 0, 0, 255),
                            dimensions: TextDimensions::Fitted {
                                max_width: 7682.0,
                                max_height: 4320.0,
                            },
                            ..Default::default()
                        }),
                        Component::View(ViewComponent::default()),
                    ],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        })
    };
    runner.update_scene(Component::Tiles(TilesComponent {
        margin: 10.0,
        children: vec![labeled_tile(1), labeled_tile(2), labeled_tile(3)],
        ..Default::default()
    }));
    runner.snapshot(Duration::ZERO);
    runner.finish()
}
