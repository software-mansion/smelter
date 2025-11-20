use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use smelter_render::Resolution;
use test_case::{OUTPUT_ID, Step, TestCase, TestResult};

use crate::paths::render_snapshots_dir_path;

mod input;
mod snapshot;
mod test_case;
mod utils;

mod image_tests;
mod rescaler_tests;
mod shader_tests;
mod simple_tests;
mod text_tests;
mod tiles_tests;
mod tiles_transitions_tests;
mod transition_tests;
mod view_tests;
mod yuv_tests;

const DEFAULT_RESOLUTION: Resolution = Resolution {
    width: 640,
    height: 360,
};

struct TestRunner {
    cases: Vec<TestCase>,
    snapshot_dir: PathBuf,
}

impl TestRunner {
    fn new(snapshot_dir: PathBuf) -> Self {
        Self {
            cases: Vec::new(),
            snapshot_dir,
        }
    }

    fn add(&mut self, case: TestCase) {
        self.cases.push(case)
    }

    fn run(self) {
        check_test_names_uniqueness(&self.cases);
        check_unused_snapshots(&self.cases, &self.snapshot_dir);
        let has_only = self.cases.iter().any(|test| test.only);

        let mut failed = false;
        for test in self.cases.iter() {
            if has_only && !test.only {
                continue;
            }
            println!("Test \"{}\"", test.name);
            if let TestResult::Failure = test.run() {
                failed = true;
            }
        }
        if failed {
            panic!("Test failed")
        }
    }
}

fn test_steps_from_scene(scene: &'static str) -> Vec<Step> {
    vec![
        Step::UpdateSceneJson(scene),
        Step::RenderWithSnapshot(Duration::ZERO),
    ]
}

fn test_steps_from_scenes(scenes: &[&'static str]) -> Vec<Step> {
    let mut steps = scenes
        .iter()
        .copied()
        .map(Step::UpdateSceneJson)
        .collect::<Vec<_>>();
    steps.push(Step::RenderWithSnapshot(Duration::ZERO));

    steps
}

fn check_test_names_uniqueness(tests: &[TestCase]) {
    let mut test_names = HashSet::new();
    for test in tests.iter() {
        if !test_names.insert(test.name) {
            panic!(
                "Multiple snapshots tests with the same name: \"{}\".",
                test.name
            );
        }
    }
}

fn snapshot_save_path(test_name: &str, pts: &Duration) -> PathBuf {
    let pts = pts.as_millis();

    // Pad timestamp with 0s on the left to the summaric length of 5.
    let out_file_name = format!("{test_name}_{pts:05}_{OUTPUT_ID}.png");
    render_snapshots_dir_path().join(out_file_name)
}

fn check_unused_snapshots(tests: &[TestCase], snapshot_dir: &Path) {
    let existing_snapshots = tests
        .iter()
        .flat_map(TestCase::snapshot_paths)
        .collect::<HashSet<_>>();
    let mut unused_snapshots = Vec::new();
    for entry in fs::read_dir(snapshot_dir).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_name().to_string_lossy().ends_with(".png") {
            continue;
        }

        if !existing_snapshots.contains(&entry.path()) {
            unused_snapshots.push(entry.path())
        }
    }

    if !unused_snapshots.is_empty() {
        if cfg!(feature = "update_snapshots") {
            for snapshot_path in unused_snapshots {
                println!("DELETE: Unused snapshot {snapshot_path:?}");
                fs::remove_file(snapshot_path).unwrap();
            }
        } else {
            panic!("Some snapshots were not used: {unused_snapshots:#?}")
        }
    }
}
