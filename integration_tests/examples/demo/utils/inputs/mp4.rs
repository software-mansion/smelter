use crate::utils::inputs::InputHandler;
use anyhow::Result;

#[derive(Debug)]
pub struct Mp4Input {
    name: String,
}

impl Mp4Input {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
        })
    }
}

impl InputHandler for Mp4Input {
    fn name(&self) -> &str {
        &self.name
    }
}
