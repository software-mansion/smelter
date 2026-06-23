use std::{sync::Arc, time::Duration};

use super::{
    OUTPUT_ID,
    input::TestInput,
    save_dumps_env_set,
    snapshot::Snapshot,
    utils::{create_renderer, frame_to_rgba},
};

use anyhow::Result;
use smelter_render::{
    Frame, FrameSet, InputId, OutputFrameFormat, OutputId, Renderer, RendererId,
    RendererSpec, RenderingMode, Resolution, scene::Component,
};

pub(crate) struct TestRunner {
    module: &'static str,
    test_name: &'static str,
    renderer: Renderer,
    inputs: Vec<TestInput>,
    allowed_error: f32,
    resolution: Resolution,
    output_format: OutputFrameFormat,
    failed: bool,
}

impl TestRunner {
    pub(crate) fn new(module: &'static str, test_name: &'static str) -> Self {
        Self {
            module,
            test_name,
            renderer: create_renderer(RenderingMode::GpuOptimized),
            inputs: Vec::new(),
            allowed_error: 1.0,
            resolution: Resolution { width: 640, height: 360 },
            output_format: OutputFrameFormat::PlanarYuv420Bytes,
            failed: false,
        }
    }

    pub(crate) fn with_inputs(mut self, inputs: Vec<TestInput>) -> Self {
        for input in &inputs {
            self.renderer.register_input(InputId(Arc::from(input.name.clone())));
        }
        self.inputs = inputs;
        self
    }

    pub(crate) fn with_renderers(
        self,
        renderers: Vec<(RendererId, RendererSpec)>,
    ) -> Self {
        for (id, spec) in renderers {
            self.renderer.register_renderer(id, spec).unwrap();
        }
        self
    }

    pub(crate) fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub(crate) fn with_rendering_mode(mut self, mode: RenderingMode) -> Self {
        self.renderer = create_renderer(mode);
        self
    }

    pub(crate) fn update_scene(&mut self, scene: Component) {
        self.renderer
            .update_scene(
                OutputId(OUTPUT_ID.into()),
                self.resolution,
                self.output_format,
                scene,
            )
            .unwrap();
    }

    pub(crate) fn snapshot(&mut self, pts: Duration) {
        let frame = self.render_frame(pts).unwrap();
        let data = frame_to_rgba(&frame);
        let snapshot = Snapshot {
            module: self.module.to_owned(),
            test_name: self.test_name.to_owned(),
            pts,
            resolution: frame.resolution,
            data,
        };

        let diff = snapshot.diff_with_saved();
        if diff > 0.0 {
            println!(
                "Snapshot error in range (allowed: {}, current: {})",
                self.allowed_error, diff
            );
        }
        if diff > self.allowed_error {
            println!(
                "FAILED: \"{}/{}\" (pts: {}ms)",
                self.module,
                self.test_name,
                pts.as_millis()
            );
            snapshot.write_as_failed_snapshot();
            self.failed = true;
        } else if save_dumps_env_set() {
            snapshot.write_as_failed_snapshot();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn render(&mut self, pts: Duration) {
        self.render_frame(pts).unwrap();
    }

    pub(crate) fn finish(self) -> Result<()> {
        if self.failed {
            anyhow::bail!("Snapshot test \"{}/{}\" failed", self.module, self.test_name);
        }
        Ok(())
    }

    fn render_frame(&mut self, pts: Duration) -> Result<Frame> {
        let mut frame_set = FrameSet::new(pts);
        for input in &self.inputs {
            let input_id = InputId::from(Arc::from(input.name.clone()));
            let frame =
                Frame { data: input.data.clone(), resolution: input.resolution, pts };
            frame_set.frames.insert(input_id, frame);
        }

        let mut outputs = self.renderer.render(frame_set)?;
        let output_frame = outputs
            .frames
            .remove(&OutputId(OUTPUT_ID.into()))
            .expect("No scene update provided before render");
        Ok(output_frame)
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum Step {
    UpdateScene(Component),
    RenderWithSnapshot(Duration),
    Render(Duration),
}

#[cfg(test)]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TestCase {
    pub module: &'static str,
    pub test_name: &'static str,
    pub inputs: Vec<TestInput>,
    pub renderers: Vec<(RendererId, RendererSpec)>,
    pub steps: Vec<Step>,
    pub allowed_error: f32,
    pub resolution: Resolution,
    pub output_format: OutputFrameFormat,
    pub rendering_mode: RenderingMode,
}

#[cfg(test)]
impl Default for TestCase {
    fn default() -> Self {
        Self {
            module: "",
            test_name: "",
            inputs: Vec::new(),
            renderers: Vec::new(),
            steps: Vec::new(),
            allowed_error: 1.0,
            resolution: Resolution { width: 640, height: 360 },
            output_format: OutputFrameFormat::PlanarYuv420Bytes,
            rendering_mode: RenderingMode::GpuOptimized,
        }
    }
}

#[cfg(test)]
impl TestCase {
    fn renderer(&self) -> Renderer {
        let renderer = create_renderer(self.rendering_mode);
        for (id, spec) in self.renderers.iter() {
            renderer.register_renderer(id.clone(), spec.clone()).unwrap();
        }

        for input in self.inputs.iter() {
            renderer.register_input(InputId(Arc::from(input.name.clone())))
        }

        renderer
    }

    pub(crate) fn generate_snapshots(&self) -> Vec<Snapshot> {
        let mut renderer = self.renderer();
        let mut snapshots = Vec::new();

        for action in self.steps.clone() {
            match action {
                Step::UpdateScene(scene) => renderer
                    .update_scene(
                        OutputId(OUTPUT_ID.into()),
                        self.resolution,
                        self.output_format,
                        scene,
                    )
                    .unwrap(),
                Step::RenderWithSnapshot(pts) => {
                    snapshots.push(self.render_with_snaphot(&mut renderer, pts).unwrap());
                }
                Step::Render(pts) => {
                    self.render(&mut renderer, pts).unwrap();
                }
            }
        }

        snapshots
    }

    fn render_with_snaphot(
        &self,
        renderer: &mut Renderer,
        pts: Duration,
    ) -> Result<Snapshot> {
        let output_frame = self.render(renderer, pts)?;
        let new_snapshot = frame_to_rgba(&output_frame);
        Ok(Snapshot {
            module: self.module.to_owned(),
            test_name: self.test_name.to_owned(),
            pts,
            resolution: output_frame.resolution,
            data: new_snapshot,
        })
    }

    fn render(&self, renderer: &mut Renderer, pts: Duration) -> Result<Frame> {
        let mut frame_set = FrameSet::new(pts);
        for input in self.inputs.iter() {
            let input_id = InputId::from(Arc::from(input.name.clone()));
            let frame =
                Frame { data: input.data.clone(), resolution: input.resolution, pts };
            frame_set.frames.insert(input_id, frame);
        }

        let mut outputs = renderer.render(frame_set)?;
        let output_frame = outputs
            .frames
            .remove(&OutputId(OUTPUT_ID.into()))
            .expect("No scene update provided before render");
        Ok(output_frame)
    }
}
