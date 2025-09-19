use smelter_render::{scene::RGBColor, Resolution};

#[derive(Debug, Clone)]
pub struct TestInput {
    pub resolution: Resolution,
    pub data: Vec<u8>,
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
        let primary_color = Self::COLOR_VARIANTS[index];
        let secondary_color = Self::COLOR_VARIANTS[(index + 6) % 17];
        let mut data = vec![0; resolution.width * resolution.height * 4];

        let color_for_pixel = |x: usize, y: usize| {
            const BORDER_SIZE: usize = 18;
            const GRID_SIZE: usize = 72;

            let is_border_in_x =
                x <= BORDER_SIZE || (x <= resolution.width && x >= resolution.width - BORDER_SIZE);
            let is_border_in_y: bool = y <= BORDER_SIZE
                || (y <= resolution.height && y >= resolution.height - BORDER_SIZE);
            let is_on_grid = (x / GRID_SIZE + y / GRID_SIZE).is_multiple_of(2);

            if is_border_in_x || is_border_in_y || is_on_grid {
                secondary_color
            } else {
                primary_color
            }
        };

        for x_coord in 0..resolution.width {
            for y_coord in 0..resolution.height {
                let RGBColor(r, g, b) = color_for_pixel(x_coord, y_coord);

                data[(y_coord * resolution.width + x_coord) * 4] = r;
                data[(y_coord * resolution.width + x_coord) * 4 + 1] = g;
                data[(y_coord * resolution.width + x_coord) * 4 + 2] = b;
                data[(y_coord * resolution.width + x_coord) * 4 + 3] = u8::MAX;
            }
        }

        Self { resolution, data }
    }
}
