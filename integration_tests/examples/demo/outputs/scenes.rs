use serde_json::json;

use crate::inputs::InputHandler;

pub enum Scene {
    Tiles,
    PrimaryLeft,
}

impl Scene {
    fn tiles(&self, id: &str, inputs: serde_json::Value) -> serde_json::Value {
        json!({
            "type": "tiles",
            "id": id,
            "transition": {
                "duration_ms": 500,
            },
            "children": inputs,
        })
    }

    fn primary_left(&self, id: &str, inputs: serde_json::Value) -> serde_json::Value {
        todo!()
    }

    pub fn serialize(&self, id: &str, inputs: serde_json::Value) -> serde_json::Value {
        match self {
            Self::Tiles => self.tiles(id, inputs),
            Self::PrimaryLeft => self.primary_left(id, inputs),
        }
    }
}
