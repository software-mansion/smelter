use crate::utils::inputs::InputHandler;
use anyhow::Result;

pub struct WhipInput {
    name: String,
}

impl WhipInput {
    pub fn setup() -> Result<Self> {
        Ok(Self {
            name: "dummy".to_string(),
        })
    }
}

impl InputHandler for WhipInput {
    fn name(&self) -> &str {
        &self.name
    }
    fn setup_video(&mut self) -> Result<()> {
        Ok(())
    }

    fn setup_audio(&mut self) -> Result<()> {
        Ok(())
    }
}
