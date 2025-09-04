use serde_json::json;

use crate::{inputs::InputHandler, outputs::VideoResolution};

#[derive(Debug)]
pub enum Scene {
    Tiles,
    PrimaryLeft,
}

impl Scene {
    fn tiles(
        &self,
        id: &str,
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
            "id": id,
            "transition": {
                "duration_ms": 500,
            },
            "children": input_json,
        })
    }

    fn primary_left(
        &self,
        id: &str,
        inputs: &[&dyn InputHandler],
        output_name: &str,
        resolution: VideoResolution,
    ) -> serde_json::Value {
        let column_width = resolution.width / 10;
        json!({
            "type": "view",
            "children": [
                {
                    "type": "view",
                    "children": [
                        {
                            "type": "rescaler",
                        }
                    ]
                },
                {
                    "type": "view",
                    "direction": "column",

                }
            ]
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
