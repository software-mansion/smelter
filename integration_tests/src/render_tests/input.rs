use smelter_render::{scene::RGBColor, FrameData, Resolution, YuvPlanes};

#[derive(Debug, Clone)]
pub(super) struct TestInput {
    pub name: String,
    pub resolution: Resolution,
    pub data: FrameData,
}

impl TestInput {
    const COLOR_VARIANTS: [RGBColor; 17] = [
        // RED, input_0
        RGBColor(255, 0, 0),
        // GREEN, input_1
        RGBColor(0, 255, 0),
        // YELLOW, input_2
        RGBColor(255, 255, 0),
        // MAGENTA, input_3
        RGBColor(255, 0, 255),
        // BLUE, input_4
        RGBColor(0, 0, 255),
        // CYAN, input_5
        RGBColor(0, 255, 255),
        // ORANGE, input_6
        RGBColor(255, 165, 0),
        // WHITE, input_7
        RGBColor(255, 255, 255),
        // GRAY, input_8
        RGBColor(128, 128, 128),
        // LIGHT_RED, input_9
        RGBColor(255, 128, 128),
        // LIGHT_BLUE, input_10
        RGBColor(128, 128, 255),
        // LIGHT_GREEN, input_11
        RGBColor(128, 255, 128),
        // PINK, input_12
        RGBColor(255, 192, 203),
        // PURPLE, input_13
        RGBColor(128, 0, 128),
        // BROWN, input_14
        RGBColor(165, 42, 42),
        // YELLOW_GREEN, input_15
        RGBColor(154, 205, 50),
        // LIGHT_YELLOW, input_16
        RGBColor(255, 255, 224),
    ];

    pub fn new(index: usize) -> Self {
        Self::new_with_resolution(
            index,
            Resolution {
                width: 640,
                height: 360,
            },
        )
    }

    pub fn new_with_resolution(index: usize, resolution: Resolution) -> Self {
        let color = Self::COLOR_VARIANTS[index].to_yuv();
        let mut y_plane = vec![0; resolution.width * resolution.height];
        let mut u_plane = vec![0; (resolution.width * resolution.height) / 4];
        let mut v_plane = vec![0; (resolution.width * resolution.height) / 4];

        let yuv_color = |x: usize, y: usize| {
            const BORDER_SIZE: usize = 18;
            const GRID_SIZE: usize = 72;

            let is_border_in_x =
                x <= BORDER_SIZE || (x <= resolution.width && x >= resolution.width - BORDER_SIZE);
            let is_border_in_y: bool = y <= BORDER_SIZE
                || (y <= resolution.height && y >= resolution.height - BORDER_SIZE);
            let is_on_grid = (x / GRID_SIZE + y / GRID_SIZE) % 2 == 0;

            let mut y = color.0;
            if is_border_in_x || is_border_in_y || is_on_grid {
                y -= 0.2;
            }

            (y.clamp(0.0, 1.0), color.1, color.2)
        };

        for x_coord in 0..resolution.width {
            for y_coord in 0..resolution.height {
                let (y, u, v) = yuv_color(x_coord, y_coord);
                if x_coord % 2 == 0 && y_coord % 2 == 0 {
                    let (_, u2, v2) = yuv_color(x_coord + 1, y_coord);
                    let (_, u3, v3) = yuv_color(x_coord, y_coord + 1);
                    let (_, u4, v4) = yuv_color(x_coord + 1, y_coord + 1);

                    let coord = (y_coord / 2) * (resolution.width / 2) + (x_coord / 2);
                    u_plane[coord] = ((u + u2 + u3 + u4) * 64.0) as u8;
                    v_plane[coord] = ((v + v2 + v3 + v4) * 64.0) as u8;
                }

                y_plane[y_coord * resolution.width + x_coord] = (y * 255.0) as u8;
            }
        }

        let data = FrameData::PlanarYuv420(YuvPlanes {
            y_plane: y_plane.into(),
            u_plane: u_plane.into(),
            v_plane: v_plane.into(),
        });

        Self {
            name: format!("input_{index}"),
            resolution,
            data,
        }
    }
}
