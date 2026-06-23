use std::{fs, ops::ControlFlow, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use inquire::{InquireError, Select};
use integration_tests::{
    paths::{pipeline_tests_workdir, submodule_root_path},
    pipeline_tests::{PipelineTest, pipeline_tests},
    tools::{mp4_player, rtp_player},
};
use tracing::{error, info, warn};

use crate::{RunOptions, run_test};

pub(crate) fn run_all() -> Result<()> {
    let workdir = pipeline_tests_workdir();
    if workdir.exists() {
        fs::remove_dir_all(&workdir)
            .with_context(|| format!("Failed to clean {}", workdir.display()))?;
    }

    let status = run_test("test(/pipeline_tests/)", RunOptions::default())?;
    if status.success() {
        return Ok(());
    }

    let tests_with_dumps = discover_pipeline_tests_with_dumps()?;
    if tests_with_dumps.is_empty() {
        warn!("Test run failed but no test results were left in the workdir");
        return Ok(());
    }

    info!("{} test result(s) to audit", tests_with_dumps.len());
    for test in tests_with_dumps {
        let filter = pipeline_test_filter(test);
        info!("Auditing test result: {}", test.full_test_name);
        match pipeline_test_result_action_loop(test, &filter)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => break,
        }
    }
    Ok(())
}

pub(crate) fn run_specific() -> Result<()> {
    let mut tests: Vec<&'static PipelineTest> = pipeline_tests();
    tests.sort_by_key(|t| t.full_test_name);

    let labels: Vec<String> =
        tests.iter().map(|t| t.full_test_name.to_string()).collect();

    let matcher = SkimMatcherV2::default();
    let scorer = move |filter: &str, _value: &String, string_value: &str, _idx: usize| {
        if filter.is_empty() {
            Some(0)
        } else {
            matcher.fuzzy_match(string_value, filter)
        }
    };
    println!();
    let selected_idx = match Select::new("Select a pipeline test:", labels)
        .with_scorer(&scorer)
        .with_page_size(15)
        .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let test = tests[selected_idx];
    let filter = pipeline_test_filter(test);
    // Clear results from previous runs so the audit menu reflects
    // only what this run produced.
    if let Err(e) = clear_pipeline_test_dumps(test) {
        error!("Failed to clear previous test dumps: {e:#}");
    }
    // Single-test runs always preserve dumps so the audit menu is
    // available regardless of pass/fail — the user explicitly asked
    // to look at this one.
    let _status = run_test(&filter, RunOptions { save_dumps: true })?;
    let _ = pipeline_test_result_action_loop(test, &filter)?;
    Ok(())
}

/// Walk every test result already sitting in the work directory and
/// hand each one to the audit UI. Lets the user re-open the inspector
/// after a previous `audit_tests` session, or browse results produced
/// by a separate `cargo test` invocation, without having to re-run
/// the (possibly long) test. Inside each iteration the user can
/// `Skip` to advance to the next result; cancelling out of the audit
/// prompt stops the walk.
pub(crate) fn audit_existing_pipeline() -> Result<()> {
    let mut tests = discover_pipeline_tests_with_dumps()?;
    if tests.is_empty() {
        warn!("No test results found in {}", pipeline_tests_workdir().display());
        return Ok(());
    }
    tests.sort_by_key(|t| t.full_test_name);
    info!("{} test result(s) to audit", tests.len());
    for test in tests {
        let filter = pipeline_test_filter(test);
        info!("Auditing test result: {}", test.full_test_name);
        match pipeline_test_result_action_loop(test, &filter)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => break,
        }
    }
    Ok(())
}

/// Things you can do once a test has produced (or already had on disk)
/// a pair of dumps in the workdir. The dumps don't necessarily come
/// from a failed run — `audit_existing_pipeline` or `SMELTER_SAVE_DUMPS=1`
/// also surface dumps for passing tests.
#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum PipelineTestResultAction {
    #[strum(to_string = "Compare (launch inspector)")]
    Compare,
    #[strum(to_string = "Play actual")]
    PlayActual,
    #[strum(to_string = "Play expected")]
    PlayExpected,
    #[strum(to_string = "Update snapshot from actual")]
    UpdateSnapshot,
    #[strum(to_string = "Rerun test")]
    Rerun,
    #[strum(to_string = "Skip")]
    Skip,
}

fn discover_pipeline_tests_with_dumps() -> Result<Vec<&'static PipelineTest>> {
    let workdir = pipeline_tests_workdir();
    let mut found: Vec<&'static PipelineTest> = Vec::new();
    if !workdir.exists() {
        return Ok(found);
    }
    let tests = pipeline_tests();
    for entry in fs::read_dir(&workdir)
        .with_context(|| format!("Failed to read {}", workdir.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        // A test can leave just an `actual_dump_` (e.g. first run of a new
        // test with no committed snapshot) or just an `expected_dump_` (if
        // the actual dump could not be produced). Match both prefixes.
        let Some(snapshot) = name
            .strip_prefix("actual_dump_")
            .or_else(|| name.strip_prefix("expected_dump_"))
        else {
            continue;
        };
        let Some(test) = tests.iter().find(|t| t.snapshot_name == snapshot).copied()
        else {
            warn!("No registered PipelineTest matches snapshot {snapshot}");
            continue;
        };
        if !found.iter().any(|t| std::ptr::eq(*t, test)) {
            found.push(test);
        }
    }
    Ok(found)
}

fn pipeline_test_filter(test: &PipelineTest) -> String {
    let name = test
        .full_test_name
        .strip_prefix("integration_tests::")
        .unwrap_or(test.full_test_name);
    format!("test(={name})")
}

fn pipeline_test_result_action_loop(
    test: &PipelineTest,
    filter: &str,
) -> Result<ControlFlow<()>> {
    loop {
        match prompt_pipeline_test_result_action(test)? {
            Some(PipelineTestResultAction::PlayActual) => {
                if let Err(e) = play_dump(test, DumpKind::Actual) {
                    error!("Failed to play actual dump: {e:#}");
                }
            }
            Some(PipelineTestResultAction::PlayExpected) => {
                if let Err(e) = play_dump(test, DumpKind::Expected) {
                    error!("Failed to play expected dump: {e:#}");
                }
            }
            Some(PipelineTestResultAction::Compare) => {
                if let Err(e) = compare_pipeline_dumps(test) {
                    error!("Failed to inspect dumps: {e:#}");
                }
            }
            Some(PipelineTestResultAction::UpdateSnapshot) => {
                // Stay in the menu: the user may still want to rerun
                // the test against the freshly updated snapshot or
                // keep inspecting; Skip moves on.
                if let Err(e) = update_pipeline_test_snapshot(test) {
                    error!("Failed to update snapshot: {e:#}");
                }
            }
            Some(PipelineTestResultAction::Rerun) => {
                if let Err(e) = clear_pipeline_test_dumps(test) {
                    error!("Failed to clear previous test dumps: {e:#}");
                }
                // A rerun targets a single test, so always preserve
                // its dumps for the inspector regardless of pass/fail.
                match run_test(filter, RunOptions { save_dumps: true }) {
                    Ok(s) if s.success() => {
                        info!(
                            "Test passed on rerun. Choose Skip to move on, or rerun again."
                        );
                    }
                    Ok(_) => {}
                    Err(e) => error!("Failed to rerun test: {e:#}"),
                }
            }
            Some(PipelineTestResultAction::Skip) => return Ok(ControlFlow::Continue(())),
            None => return Ok(ControlFlow::Break(())),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DumpKind {
    Actual,
    Expected,
}

impl DumpKind {
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Actual => "actual_dump_",
            Self::Expected => "expected_dump_",
        }
    }
}

fn prompt_pipeline_test_result_action(
    test: &PipelineTest,
) -> Result<Option<PipelineTestResultAction>> {
    use strum::IntoEnumIterator;
    const BOLD: &str = "\x1b[1m";
    const CYAN: &str = "\x1b[36m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    println!();
    println!(
        "{BOLD}{YELLOW}── Test result ──────────────────────────────────────{RESET}"
    );
    println!("{BOLD}{CYAN}{}{RESET}", test.full_test_name);
    if !test.description.is_empty() {
        println!("{}", test.description);
    }
    println!(
        "{BOLD}{YELLOW}─────────────────────────────────────────────────────{RESET}"
    );
    println!();

    let actual_exists = pipeline_tests_workdir()
        .join(format!("actual_dump_{}", test.snapshot_name))
        .exists();
    let expected_exists = pipeline_tests_workdir()
        .join(format!("expected_dump_{}", test.snapshot_name))
        .exists();
    let options: Vec<PipelineTestResultAction> = PipelineTestResultAction::iter()
        .filter(|a| match a {
            PipelineTestResultAction::UpdateSnapshot => actual_exists,
            // Inspector tolerates either side missing now, so as long
            // as we have anything at all to look at, offer it.
            PipelineTestResultAction::Compare => actual_exists || expected_exists,
            PipelineTestResultAction::PlayExpected => expected_exists,
            PipelineTestResultAction::PlayActual => actual_exists,
            _ => true,
        })
        .collect();
    match Select::new("What next?", options).with_page_size(10).prompt() {
        Ok(a) => Ok(Some(a)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

fn clear_pipeline_test_dumps(test: &PipelineTest) -> Result<()> {
    let dir = pipeline_tests_workdir();
    for prefix in ["actual_dump_", "expected_dump_"] {
        let path = dir.join(format!("{prefix}{}", test.snapshot_name));
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

fn play_dump(test: &PipelineTest, kind: DumpKind) -> Result<()> {
    let path = pipeline_tests_workdir().join(format!(
        "{}{}",
        kind.file_prefix(),
        test.snapshot_name
    ));
    if !path.exists() {
        warn!("Dump not found: {}", path.display());
        return Ok(());
    }
    info!("Press Esc or q to stop playback");
    let child = if test.snapshot_name.ends_with(".mp4") {
        mp4_player::spawn(&path)?
    } else {
        rtp_player::spawn(&path)?
    };
    run_with_kill_on_key(child)
}

/// Watches our own stdin in raw mode for Esc or `q`. On either key,
/// sends SIGINT to the child's process group so the whole subtree
/// (bash → gst-launch and any of its workers) shuts down, then reaps
/// the child.
fn run_with_kill_on_key(mut child: std::process::Child) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode};

    let child_pgid = child.id() as libc::pid_t;

    crossterm::terminal::enable_raw_mode()?;
    let result: Result<std::process::ExitStatus> = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(e.into()),
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => break Err(e.into()),
        }
        match event::read() {
            Ok(Event::Key(key))
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) =>
            {
                // SAFETY: sending a signal to a process group has no Rust-side
                // invariants. The group was created when we spawned the child.
                unsafe { libc::kill(-child_pgid, libc::SIGINT) };
                break child.wait().map_err(anyhow::Error::from);
            }
            Ok(_) => {}
            Err(e) => break Err(e.into()),
        }
    };
    let _ = crossterm::terminal::disable_raw_mode();
    // Child output ran under our raw mode (LF-only) and gst-launch may have
    // emitted control sequences before SIGINT; restore sane TTY state.
    let _ = std::process::Command::new("stty").arg("sane").status();

    let status = result?;
    if !status.success() {
        warn!("play_rtp_dump exited with {status}");
    }
    Ok(())
}

fn compare_pipeline_dumps(test: &PipelineTest) -> Result<()> {
    let dir = pipeline_tests_workdir();
    let actual = dir.join(format!("actual_dump_{}", test.snapshot_name));
    let expected = dir.join(format!("expected_dump_{}", test.snapshot_name));

    crate::pipeline_tests_inspector::run(&expected, &actual)
}

fn update_pipeline_test_snapshot(test: &PipelineTest) -> Result<()> {
    let src =
        pipeline_tests_workdir().join(format!("actual_dump_{}", test.snapshot_name));
    if !src.exists() {
        anyhow::bail!("No actual dump at {}", src.display());
    }
    let dst = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("outputs")
        .join(test.snapshot_name);
    fs::copy(&src, &dst).with_context(|| {
        format!("Failed to copy {} -> {}", src.display(), dst.display())
    })?;
    info!("Updated {}", dst.display());
    Ok(())
}

pub(crate) fn find_orphan_pipeline_snapshots() -> Result<Vec<PathBuf>> {
    let root = submodule_root_path().join("rtp_packet_dumps").join("outputs");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let used: std::collections::HashSet<&'static str> =
        pipeline_tests().iter().map(|t| t.snapshot_name).collect();
    let mut orphans: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&root)
        .with_context(|| format!("Failed to read {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !used.contains(name) {
            orphans.push(path);
        }
    }
    orphans.sort();
    Ok(orphans)
}
