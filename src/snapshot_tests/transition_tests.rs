use std::time::Duration;

use super::{
    snapshots_path,
    test_case::{Step, TestCase},
    TestRunner,
};

#[test]
fn transitions_tests() {
    let mut runner = TestRunner::new(snapshots_path().join("transition"));
    let render_timestamps = vec![
        Step::RenderWithSnapshot(Duration::from_millis(0)),
        Step::RenderWithSnapshot(Duration::from_millis(2500)),
        Step::RenderWithSnapshot(Duration::from_millis(5000)),
        Step::RenderWithSnapshot(Duration::from_millis(7500)),
        Step::RenderWithSnapshot(Duration::from_millis(9000)),
        Step::RenderWithSnapshot(Duration::from_millis(10000)),
    ];

    runner.add(TestCase {
        name: "transition/change_rescaler_absolute_and_send_next_update",
        steps: [
            [
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_rescaler_absolute_start.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_rescaler_absolute_end.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_rescaler_absolute_after_end.scene.json"
                )),
            ]
            .as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_width_and_send_abort_transition",
        steps: [
            [
                Step::UpdateSceneJson(include_str!("../../snapshot_tests/transition/change_view_width_start.scene.json")),
                Step::UpdateSceneJson(include_str!("../../snapshot_tests/transition/change_view_width_end.scene.json")),
                Step::UpdateSceneJson(include_str!("../../snapshot_tests/transition/change_view_width_after_end_without_id.scene.json")),
            ].as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_width_and_send_next_update",
        steps: [
            [
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_width_start.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_width_end.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_width_after_end.scene.json"
                )),
            ]
            .as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_width",
        steps: [
            [
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_width_start.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_width_end.scene.json"
                )),
            ]
            .as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_height",
        steps: [
            [
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_height_start.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_height_end.scene.json"
                )),
            ]
            .as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_absolute",
        steps: [
            [
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_absolute_start.scene.json"
                )),
                Step::UpdateSceneJson(include_str!(
                    "../../snapshot_tests/transition/change_view_absolute_end.scene.json"
                )),
            ]
            .as_slice(),
            &render_timestamps,
        ]
        .concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_absolute_cubic_bezier",
        steps: [
            [
                Step::UpdateSceneJson( include_str!("../../snapshot_tests/transition/change_view_absolute_cubic_bezier_start.scene.json")),
                Step::UpdateSceneJson( include_str!("../../snapshot_tests/transition/change_view_absolute_cubic_bezier_end.scene.json")),
            ].as_slice(),
            &render_timestamps,
        ].concat(),
        ..Default::default()
    });
    runner.add(TestCase {
        name: "transition/change_view_absolute_cubic_bezier_linear_like",
        steps: [
            [
                Step::UpdateSceneJson(include_str!("../../snapshot_tests/transition/change_view_absolute_cubic_bezier_linear_like_start.scene.json")),
                Step::UpdateSceneJson( include_str!("../../snapshot_tests/transition/change_view_absolute_cubic_bezier_linear_like_end.scene.json")),
            ].as_slice(),
            &render_timestamps
        ]
        .concat(),
        ..Default::default()
    });

    runner.run()
}
