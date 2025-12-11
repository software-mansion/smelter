use core::panic;
use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use smelter_render::{
    FrameData, InputId, OutputFrameFormat, Resolution,
    scene::{Component, InputStreamComponent, ViewComponent},
};

use crate::render_tests::input::TestInput;

use super::{Step, test_case::TestCase};

fn run_case(test_case: TestCase, expected: &[u8]) {
    let snapshots = test_case.generate_snapshots();
    let failed = snapshots[0]
        .data
        .iter()
        .zip(expected)
        .any(|(actual, expected)| actual != expected);
    if failed {
        panic!(
            "Sample mismatched actual: {:?}, expected: {:?}",
            snapshots[0].data, expected
        )
    }
}

#[test]
fn test_bgra_pixel_format_input() {
    let width = 8;
    let height = 2;
    let input_id = "input";

    #[rustfmt::skip]
    let input_data = &[
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
        33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
    ];

    let input_component = Component::InputStream(InputStreamComponent {
        id: None,
        input_id: InputId::from(Arc::from(input_id)),
    });

    let view_component = Component::View(ViewComponent {
        children: vec![input_component],
        ..Default::default()
    });

    let input_frame = TestInput {
        name: input_id.to_string(),
        resolution: Resolution { width, height },
        data: FrameData::Bgra(Bytes::from_static(input_data)),
    };
    let case = TestCase {
        output_format: OutputFrameFormat::RgbaWgpuTexture,
        resolution: Resolution { width, height },
        steps: vec![
            Step::UpdateScene(view_component),
            Step::RenderWithSnapshot(Duration::ZERO),
        ],
        inputs: vec![input_frame],
        ..Default::default()
    };

    #[rustfmt::skip]
    run_case(case,
        &[
            3, 2, 1, 4, 
            7, 6, 5, 8,
            11, 10, 9, 12, 
            15, 14, 13, 16, 
            19, 18, 17, 20, 
            23, 22, 21, 24, 
            27, 26, 25, 28, 
            31, 30, 29, 32,

            35, 34, 33, 36, 
            39, 38, 37, 40, 
            43, 42, 41, 44, 
            47, 46, 45, 48, 
            51, 50, 49, 52, 
            55, 54, 53, 56, 
            59, 58, 57, 60,
            63, 62, 61, 64
        ],
    );
}

#[test]
fn test_argb_pixel_format_input() {
    let width = 8;
    let height = 2;
    let input_id = "input";

    #[rustfmt::skip]
    let input_data = &[
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
        33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
    ];

    let input_component = Component::InputStream(InputStreamComponent {
        id: None,
        input_id: InputId::from(Arc::from(input_id)),
    });

    let view_component = Component::View(ViewComponent {
        children: vec![input_component],
        ..Default::default()
    });

    let input_frame = TestInput {
        name: input_id.to_string(),
        resolution: Resolution { width, height },
        data: FrameData::Argb(Bytes::from_static(input_data)),
    };
    let case = TestCase {
        output_format: OutputFrameFormat::RgbaWgpuTexture,
        resolution: Resolution { width, height },
        steps: vec![
            Step::UpdateScene(view_component),
            Step::RenderWithSnapshot(Duration::ZERO),
        ],
        inputs: vec![input_frame],
        ..Default::default()
    };

    #[rustfmt::skip]
    run_case(case,
        &[
            4, 1, 2, 3,
            8, 5, 6, 7,
            12, 9, 10, 11,
            16, 13, 14, 15,
            20, 17, 18, 19,
            24, 21, 22, 23,
            28, 25, 26, 27,
            32, 29, 30, 31,
            
            36, 33, 34, 35,
            40, 37, 38, 39,
            44, 41, 42, 43,
            48, 45, 46, 47,
            52, 49, 50, 51,
            56, 53, 54, 55,
            60, 57, 58, 59,
            64, 61, 62, 63
        ],
    );
}
