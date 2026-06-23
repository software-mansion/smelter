use std::time::Duration;

use anyhow::Result;
use integration_tests_macros::render_test;
use smelter_render::{
    InputId,
    scene::{
        AbsolutePosition, Component, ComponentId, HorizontalAlign, HorizontalPosition,
        InputStreamComponent, InterpolationKind, Position, RGBAColor, TilesComponent,
        Transition, VerticalPosition, ViewComponent,
    },
};

use crate::render_tests::{
    RenderTest,
    harness::{input::TestInput, test_case::TestRunner},
};

pub const TESTS: &[RenderTest] = &[
    TILE_RESIZE_ENTIRE_COMPONENT_WITH_PARENT_TRANSITION,
    TILE_RESIZE_ENTIRE_COMPONENT_WITHOUT_PARENT_TRANSITION,
    CHANGE_ORDER_OF_3_INPUTS_WITH_ID,
    REPLACE_COMPONENT_BY_ADDING_ID,
    ADD_2_INPUTS_AT_THE_END_TO_3_TILES_SCENE,
    ADD_INPUT_ON_2ND_POS_TO_3_TILES_SCENE,
    ADD_INPUT_AT_THE_END_TO_3_TILES_SCENE,
    REPLACE_COMPONENT_BY_CHANGING_ID,
    REPLACE_COMPONENT_BY_CHANGING_ID_AND_ADD_NEW_COMPONENT,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_MARGIN,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_NEW_COMPONENT_LAST_ROW_CENTER_ALIGNED,
    REPLACE_COMPONENT_BY_CHANGING_ID_ADD_NEW_COMPONENT_LAST_ROW_LEFT_ALIGNED,
];

const DARK_GRAY: RGBAColor = RGBAColor(0x33, 0x33, 0x33, 255);
const TILES_ID: &str = "tiles";

fn linear_500ms(should_interrupt: bool) -> Transition {
    Transition {
        duration: Duration::from_millis(500),
        interpolation_kind: InterpolationKind::Linear,
        should_interrupt,
    }
}

/// `input_stream` component for `input_{idx}`, optionally with the matching id.
fn input(idx: usize, with_id: bool) -> Component {
    let name = format!("input_{idx}");
    Component::InputStream(InputStreamComponent {
        id: with_id.then(|| ComponentId(name.clone().into())),
        input_id: InputId(name.into()),
    })
}

#[render_test(description = "")]
fn tile_resize_entire_component_with_parent_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    // Start: large green container at bottom:0 right:0, no transitions.
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            id: Some(ComponentId("view".into())),
            background_color: DARK_GRAY,
            position: Position::Absolute(AbsolutePosition {
                width: Some(640.0),
                height: Some(360.0),
                position_horizontal: HorizontalPosition::RightOffset(0.0),
                position_vertical: VerticalPosition::BottomOffset(0.0),
                rotation_degrees: 0.0,
            }),
            children: vec![Component::Tiles(TilesComponent {
                id: Some(ComponentId(TILES_ID.into())),
                children: vec![input(1, true), input(2, true), input(3, true)],
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    // End: shrink container to 320x340 with 500ms transition on both view and tiles.
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            id: Some(ComponentId("view".into())),
            background_color: DARK_GRAY,
            position: Position::Absolute(AbsolutePosition {
                width: Some(320.0),
                height: Some(340.0),
                position_horizontal: HorizontalPosition::RightOffset(10.0),
                position_vertical: VerticalPosition::BottomOffset(10.0),
                rotation_degrees: 0.0,
            }),
            transition: Some(linear_500ms(false)),
            children: vec![Component::Tiles(TilesComponent {
                id: Some(ComponentId(TILES_ID.into())),
                transition: Some(linear_500ms(false)),
                children: vec![input(1, true), input(2, true), input(3, true)],
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    // TODO: This transition does not look great, but it would require automatic
    // transitions triggered by a size change (not scene update)
    runner.snapshot(Duration::from_millis(400));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn tile_resize_entire_component_without_parent_transition() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    // Start: large green container at bottom:0 right:0, no transitions on the outer view.
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            id: Some(ComponentId("view".into())),
            background_color: DARK_GRAY,
            position: Position::Absolute(AbsolutePosition {
                width: Some(640.0),
                height: Some(360.0),
                position_horizontal: HorizontalPosition::RightOffset(0.0),
                position_vertical: VerticalPosition::BottomOffset(0.0),
                rotation_degrees: 0.0,
            }),
            children: vec![Component::Tiles(TilesComponent {
                id: Some(ComponentId(TILES_ID.into())),
                children: vec![input(1, true), input(2, true), input(3, true)],
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    // End: shrink container instantly (no view transition), only Tiles transitions.
    runner.update_scene(Component::View(ViewComponent {
        children: vec![Component::View(ViewComponent {
            background_color: DARK_GRAY,
            position: Position::Absolute(AbsolutePosition {
                width: Some(320.0),
                height: Some(340.0),
                position_horizontal: HorizontalPosition::RightOffset(10.0),
                position_vertical: VerticalPosition::BottomOffset(10.0),
                rotation_degrees: 0.0,
            }),
            children: vec![Component::Tiles(TilesComponent {
                id: Some(ComponentId(TILES_ID.into())),
                transition: Some(linear_500ms(false)),
                children: vec![input(1, true), input(2, true), input(3, true)],
                ..Default::default()
            })],
            ..Default::default()
        })],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn change_order_of_3_inputs_with_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: vec![input(3, true), input(1, true), input(2, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_adding_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, false), input(2, false), input(3, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: vec![input(1, false), input(4, true), input(2, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_2_inputs_at_the_end_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, false), input(2, false), input(3, false)],
        ..Default::default()
    }));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: (1..=5).map(|i| input(i, false)).collect(),
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_input_on_2nd_pos_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, false), input(2, false), input(3, false)],
        ..Default::default()
    }));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: vec![input(1, false), input(4, true), input(2, false), input(3, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn add_input_at_the_end_to_3_tiles_scene() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, false), input(2, false), input(3, false)],
        ..Default::default()
    }));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: (1..=4).map(|i| input(i, false)).collect(),
        ..Default::default()
    }));
    // Third update: same children, no transition — should "complete" the move.
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: (1..=4).map(|i| input(i, false)).collect(),
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: vec![input(1, true), input(4, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_and_add_new_component() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        children: vec![input(1, true), input(4, true), input(3, true), input(5, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_margin() -> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        margin: 50.0,
        children: vec![input(1, true), input(4, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_new_component_last_row_center_aligned()
-> Result<()> {
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        horizontal_align: HorizontalAlign::Center,
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        horizontal_align: HorizontalAlign::Center,
        children: vec![input(1, true), input(2, true), input(4, true), input(5, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}

#[render_test(description = "")]
fn replace_component_by_changing_id_add_new_component_last_row_left_aligned() -> Result<()>
{
    let mut runner = TestRunner::new(MODULE, TEST_NAME).with_inputs(vec![
        TestInput::new(1),
        TestInput::new(2),
        TestInput::new(3),
        TestInput::new(4),
        TestInput::new(5),
    ]);
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        horizontal_align: HorizontalAlign::Left,
        children: vec![input(1, true), input(2, true), input(3, true)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(0));
    runner.update_scene(Component::Tiles(TilesComponent {
        id: Some(ComponentId(TILES_ID.into())),
        transition: Some(linear_500ms(false)),
        horizontal_align: HorizontalAlign::Left,
        children: vec![input(1, true), input(2, true), input(4, true), input(5, false)],
        ..Default::default()
    }));
    runner.snapshot(Duration::from_millis(1));
    runner.snapshot(Duration::from_millis(100));
    runner.snapshot(Duration::from_millis(300));
    runner.snapshot(Duration::from_millis(500));
    runner.finish()
}
