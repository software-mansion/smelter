use std::{path::PathBuf, sync::Arc, time::Duration};

use super::{
    input::TestInput,
    snapshot::Snapshot,
    snapshot_save_path,
    utils::{create_renderer, frame_to_rgba},
};

use anyhow::Result;
use compositor_api::types::UpdateOutputRequest;
use compositor_render::{
    scene::Component, Frame, FrameSet, InputId, OutputFrameFormat, OutputId, Renderer, RendererId,
    RendererSpec, Resolution,
};

pub(super) const OUTPUT_ID: &str = "output_1";

#[derive(Debug, Clone)]
pub(super) enum Step {
    UpdateScene(Component),
    UpdateSceneJson(&'static str),
    RenderWithSnapshot(Duration),
    #[allow(dead_code)]
    Render(Duration),
}

#[derive(Debug, Clone)]
pub(super) struct TestCase {
    pub name: &'static str,
    pub inputs: Vec<TestInput>,
    pub renderers: Vec<(RendererId, RendererSpec)>,
    pub steps: Vec<Step>,
    pub only: bool,
    pub allowed_error: f32,
    pub resolution: Resolution,
    pub output_format: OutputFrameFormat,
}

impl Default for TestCase {
    fn default() -> Self {
        Self {
            name: "",
            inputs: Vec::new(),
            renderers: Vec::new(),
            only: false,
            steps: Vec::new(),
            allowed_error: 1.0,
            resolution: Resolution {
                width: 640,
                height: 360,
            },
            output_format: OutputFrameFormat::PlanarYuv420Bytes,
        }
    }
}

pub(super) enum TestResult {
    Success,
    Failure,
}

impl TestCase {
    pub(super) fn renderer(&self) -> Renderer {
        let renderer = create_renderer();
        for (id, spec) in self.renderers.iter() {
            renderer
                .register_renderer(id.clone(), spec.clone())
                .unwrap();
        }

        for (index, _) in self.inputs.iter().enumerate() {
            renderer.register_input(InputId(format!("input_{}", index + 1).into()))
        }

        renderer
    }

    pub(super) fn run(&self) -> TestResult {
        if self.name.is_empty() {
            panic!("Snapshot test name has to be provided");
        }
        let snapshots = self.generate_snapshots();
        self.verify_snapshots(snapshots)
    }

    pub(super) fn generate_snapshots(&self) -> Vec<Snapshot> {
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
                Step::UpdateSceneJson(scene) => {
                    let scene: UpdateOutputRequest = serde_json::from_str(scene).unwrap();
                    renderer
                        .update_scene(
                            OutputId(OUTPUT_ID.into()),
                            self.resolution,
                            self.output_format,
                            scene.video.unwrap().try_into().unwrap(),
                        )
                        .unwrap()
                }
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

    fn verify_snapshots(&self, snapshots: Vec<Snapshot>) -> TestResult {
        let mut result = TestResult::Success;
        for snapshot in snapshots {
            let snapshots_diff = snapshot.diff_with_saved();
            if snapshots_diff > 0.0 {
                println!(
                    "Snapshot error in range (allowed: {}, current: {})",
                    self.allowed_error, snapshots_diff
                );
            }
            if snapshots_diff > self.allowed_error {
                if cfg!(feature = "update_snapshots") {
                    println!(
                        "UPDATE: \"{}\" (pts: {}ms)",
                        self.name,
                        snapshot.pts.as_millis()
                    );
                    snapshot.update_on_disk();
                } else {
                    println!(
                        "FAILED: \"{}\" (pts: {}ms)",
                        self.name,
                        snapshot.pts.as_millis()
                    );
                    snapshot.write_as_failed_snapshot();
                    result = TestResult::Failure;
                }
            }
        }

        result
    }

    pub(super) fn snapshot_paths(&self) -> Vec<PathBuf> {
        self.steps
            .iter()
            .flat_map(|step| match step {
                Step::RenderWithSnapshot(pts) => Some(snapshot_save_path(self.name, pts)),
                _ => None,
            })
            .collect()
    }

    fn render_with_snaphot(&self, renderer: &mut Renderer, pts: Duration) -> Result<Snapshot> {
        let output_frame = self.render(renderer, pts)?;
        let new_snapshot = frame_to_rgba(&output_frame);
        Ok(Snapshot {
            test_name: self.name.to_owned(),
            pts,
            resolution: output_frame.resolution,
            data: new_snapshot,
        })
    }

    fn render(&self, renderer: &mut Renderer, pts: Duration) -> Result<Frame> {
        let mut frame_set = FrameSet::new(pts);
        for input in self.inputs.iter() {
            let input_id = InputId::from(Arc::from(input.name.clone()));
            let frame = Frame {
                data: input.data.clone(),
                resolution: input.resolution,
                pts,
            };
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
