use serde_json::json;
use strum::{Display, EnumIter};

use crate::{inputs::InputHandler, outputs::VideoResolution};

#[derive(Debug, Display, EnumIter)]
pub enum Scene {
    #[strum(to_string = "Tiles")]
    Tiles,

    #[strum(to_string = "Primary left")]
    PrimaryLeft,
}

impl Scene {
    fn tiles(
        &self,
        root_id: &str,
        inputs: &[&dyn InputHandler],
        output_name: &str,
    ) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input| {
                let input_name = input.name();
                let id = format!("{input_name}_{output_name}");
                json!({
                    "type": "input_stream",
                    "id": id,
                    "input_id": input_name,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "type": "tiles",
            "id": root_id,
            "transition": {
                "duration_ms": 500,
            },
            "children": input_json,
        })
    }

    fn primary_left(
        &self,
        root_id: &str,
        inputs: &[&dyn InputHandler],
        output_name: &str,
        resolution: VideoResolution,
    ) -> serde_json::Value {
        let primary_input = inputs
            .first()
            .map(|input| {
                let input_name = input.name();
                let id = format!("{input_name}_{output_name}");
                json!({
                    "type": "input_stream",
                    "id": id,
                    "input_id": input_name,
                })
            })
            .unwrap_or(json!({
                "type": "view",
            }));

        let column_width = resolution.width / 4;
        let input_json = inputs
            .iter()
            .skip(1)
            .map(|input| {
                let input_name = input.name();
                let id = format!("{input_name}_{output_name}");
                json!({
                    "type": "rescaler",
                    "child": {
                        "type": "input_stream",
                        "id": id,
                        "input_id": input_name,
                    },
                })
            })
            .collect::<Vec<_>>();

        json!({
            "type": "view",
            "id": root_id,
            "children": [
                {
                    "type": "view",
                    "children": [
                        {
                            "type": "rescaler",
                            "child": primary_input,
                        }
                    ],
                },
                {
                    "type": "view",
                    "direction": "column",
                    "width": column_width,
                    "children": input_json,

                },
            ],
        })
    }

    pub fn serialize(
        &self,
        id: &str,
        inputs: &[&dyn InputHandler],
        output_name: &str,
        resolution: VideoResolution,
    ) -> serde_json::Value {
        match self {
            Self::Tiles => self.tiles(id, inputs, output_name),
            Self::PrimaryLeft => self.primary_left(id, inputs, output_name, resolution),
        }
    }
}
