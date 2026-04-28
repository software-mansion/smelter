use std::{
    fs,
    ops::ControlFlow,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use inquire::{InquireError, Select};
use integration_tests::{
    paths::{failed_snapshots_dir_path, submodule_root_path},
    pipeline_tests::{PipelineTest, pipeline_tests},
    tools::rtp_inspector,
};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum Action {
    #[strum(to_string = "Run all pipeline tests")]
    RunAll,
    #[strum(to_string = "Run specific pipeline test")]
    RunSpecific,
}

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum FailureAction {
    #[strum(to_string = "Play actual dump")]
    PlayActual,
    #[strum(to_string = "Play expected dump")]
    PlayExpected,
    #[strum(to_string = "Inspect dumps (compare actual vs expected)")]
    Inspect,
    #[strum(to_string = "Update snapshot from actual")]
    UpdateSnapshot,
    #[strum(to_string = "Rerun test")]
    Rerun,
    #[strum(to_string = "Skip")]
    Skip,
}

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum StreamKind {
    #[strum(to_string = "video")]
    Video,
    #[strum(to_string = "audio")]
    Audio,
    #[strum(to_string = "av (audio + video)")]
    Av,
}

impl StreamKind {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Av => "av",
        }
    }
}

fn main() -> Result<()> {
    use strum::IntoEnumIterator;

    tracing_subscriber::fmt().with_target(false).init();

    loop {
        let options: Vec<Action> = Action::iter().collect();
        let choice = match Select::new("What would you like to do?", options).prompt() {
            Ok(c) => c,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        let result = match choice {
            Action::RunAll => run_all(),
            Action::RunSpecific => run_specific(),
        };
        if let Err(e) = result {
            error!("{e:#}");
        }
    }
}

fn run_specific() -> Result<()> {
    let mut tests: Vec<&'static PipelineTest> = pipeline_tests();
    tests.sort_by_key(|t| t.full_test_name);

    let labels: Vec<String> = tests.iter().map(|t| t.full_test_name.to_string()).collect();

    let matcher = SkimMatcherV2::default();
    let scorer = move |filter: &str, _value: &String, string_value: &str, _idx: usize| {
        if filter.is_empty() {
            Some(0)
        } else {
            matcher.fuzzy_match(string_value, filter)
        }
    };
    let selected_idx = match Select::new("Select a pipeline test:", labels)
        .with_scorer(&scorer)
        .with_page_size(15)
        .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let test = tests[selected_idx];
    let filter = test_filter(test);
    let status = run_nextest(&[&filter])?;
    if !status.success() {
        let _ = handle_failure_loop(test, &filter)?;
    }
    Ok(())
}

fn run_all() -> Result<()> {
    let failed_dir = failed_snapshots_dir_path();
    if failed_dir.exists() {
        fs::remove_dir_all(&failed_dir)
            .with_context(|| format!("Failed to clean {}", failed_dir.display()))?;
    }

    let status = run_nextest(&["pipeline_tests"])?;
    if status.success() {
        return Ok(());
    }

    let failed_tests = discover_failed_tests()?;
    if failed_tests.is_empty() {
        warn!("Test run failed but no failed snapshot dumps were produced");
        return Ok(());
    }

    info!(
        "{} test(s) produced failed snapshot dumps",
        failed_tests.len()
    );
    for test in failed_tests {
        let filter = test_filter(test);
        info!("Handling failed test: {}", test.full_test_name);
        match handle_failure_loop(test, &filter)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => break,
        }
    }
    Ok(())
}

fn discover_failed_tests() -> Result<Vec<&'static PipelineTest>> {
    let failed_dir = failed_snapshots_dir_path();
    let mut failed: Vec<&'static PipelineTest> = Vec::new();
    if !failed_dir.exists() {
        return Ok(failed);
    }
    let tests = pipeline_tests();
    for entry in fs::read_dir(&failed_dir)
        .with_context(|| format!("Failed to read {}", failed_dir.display()))?
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
        let Some(test) = tests.iter().find(|t| t.snapshot_name == snapshot).copied() else {
            warn!("No registered PipelineTest matches snapshot {snapshot}");
            continue;
        };
        if !failed.iter().any(|t| std::ptr::eq(*t, test)) {
            failed.push(test);
        }
    }
    Ok(failed)
}

fn test_filter(test: &PipelineTest) -> String {
    test.full_test_name
        .strip_prefix("integration_tests::")
        .unwrap_or(test.full_test_name)
        .to_string()
}

fn handle_failure_loop(test: &PipelineTest, filter: &str) -> Result<ControlFlow<()>> {
    loop {
        match prompt_failure_action(test)? {
            Some(FailureAction::PlayActual) => {
                if let Err(e) = play_dump(test, DumpKind::Actual) {
                    error!("Failed to play actual dump: {e:#}");
                }
            }
            Some(FailureAction::PlayExpected) => {
                if let Err(e) = play_dump(test, DumpKind::Expected) {
                    error!("Failed to play expected dump: {e:#}");
                }
            }
            Some(FailureAction::Inspect) => {
                if let Err(e) = inspect_dumps(test) {
                    error!("Failed to inspect dumps: {e:#}");
                }
            }
            Some(FailureAction::UpdateSnapshot) => match update_snapshot(test) {
                Ok(()) => return Ok(ControlFlow::Continue(())),
                Err(e) => error!("Failed to update snapshot: {e:#}"),
            },
            Some(FailureAction::Rerun) => {
                if let Err(e) = clear_failed_dumps(test) {
                    error!("Failed to clear previous failed dumps: {e:#}");
                }
                match run_nextest(&[filter]) {
                    Ok(s) if s.success() => {
                        info!("Test passed on rerun. Choose Skip to move on, or rerun again.");
                    }
                    Ok(_) => {}
                    Err(e) => error!("Failed to rerun test: {e:#}"),
                }
            }
            Some(FailureAction::Skip) => return Ok(ControlFlow::Continue(())),
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

fn prompt_failure_action(test: &PipelineTest) -> Result<Option<FailureAction>> {
    use strum::IntoEnumIterator;
    const BOLD: &str = "\x1b[1m";
    const CYAN: &str = "\x1b[36m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    println!();
    println!("{BOLD}{YELLOW}── Failed test ──────────────────────────────────────{RESET}");
    println!("{BOLD}{CYAN}{}{RESET}", test.full_test_name);
    if !test.description.is_empty() {
        println!("{}", test.description);
    }
    println!("{BOLD}{YELLOW}─────────────────────────────────────────────────────{RESET}");
    println!();

    let actual_exists = failed_snapshots_dir_path()
        .join(format!("actual_dump_{}", test.snapshot_name))
        .exists();
    let expected_exists = failed_snapshots_dir_path()
        .join(format!("expected_dump_{}", test.snapshot_name))
        .exists();
    let options: Vec<FailureAction> = FailureAction::iter()
        .filter(|a| match a {
            FailureAction::UpdateSnapshot => actual_exists,
            // Inspector tolerates either side missing now, so as long
            // as we have anything at all to look at, offer it.
            FailureAction::Inspect => actual_exists || expected_exists,
            FailureAction::PlayExpected => expected_exists,
            FailureAction::PlayActual => actual_exists,
            _ => true,
        })
        .collect();
    match Select::new("What next?", options)
        .with_page_size(10)
        .prompt()
    {
        Ok(a) => Ok(Some(a)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn clear_failed_dumps(test: &PipelineTest) -> Result<()> {
    let dir = failed_snapshots_dir_path();
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
    let path =
        failed_snapshots_dir_path().join(format!("{}{}", kind.file_prefix(), test.snapshot_name));
    if !path.exists() {
        warn!("Dump not found: {}", path.display());
        return Ok(());
    }

    use strum::IntoEnumIterator;
    let options: Vec<StreamKind> = StreamKind::iter().collect();
    let stream_kind = match Select::new("Select stream kind:", options).prompt() {
        Ok(k) => k,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    let mut cmd = Command::new("cargo");
    scrub_cargo_env(&mut cmd);
    cmd.args([
        "run",
        "-p",
        "integration-tests",
        "--bin",
        "play_rtp_dump",
        "--",
        stream_kind.as_arg(),
    ])
    .arg(&path)
    // Child doesn't read stdin; we own stdin exclusively so our raw-mode
    // key watcher can detect Esc/q without racing the child.
    .stdin(Stdio::null())
    // Child (cargo) and all its descendants get a fresh process group so we
    // can kill them all at once by signalling the group.
    .process_group(0);

    info!("Press Esc or q to stop playback");
    run_with_kill_on_key(cmd)
}

/// Runs `cmd` while watching our own stdin in raw mode for Esc or `q`. On
/// either key, sends SIGINT to the child's process group so the whole
/// subtree (cargo → play_rtp_dump → bash → gst-launch) shuts down, then
/// reaps the child.
fn run_with_kill_on_key(mut cmd: Command) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode};

    let mut child = cmd.spawn()?;
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
            Ok(Event::Key(key)) if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) => {
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

/// Removes CARGO_* vars that `cargo run` sets for our process, so a nested
/// cargo invocation doesn't misinterpret them and invalidate fingerprints.
/// Keeps user-configured vars like `CARGO_HOME` and `CARGO_TARGET_DIR`.
fn scrub_cargo_env(cmd: &mut Command) {
    for (key, _) in std::env::vars() {
        let strip = key == "CARGO"
            || key == "OUT_DIR"
            || key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_CRATE_")
            || key.starts_with("CARGO_BIN_")
            || key.starts_with("CARGO_MANIFEST_")
            || key == "CARGO_PRIMARY_PACKAGE"
            || key == "CARGO_TARGET_TMPDIR";
        if strip {
            cmd.env_remove(&key);
        }
    }
}

fn inspect_dumps(test: &PipelineTest) -> Result<()> {
    let dir = failed_snapshots_dir_path();
    let actual = dir.join(format!("actual_dump_{}", test.snapshot_name));
    let expected = dir.join(format!("expected_dump_{}", test.snapshot_name));

    rtp_inspector::run(&expected, &actual)
}

fn update_snapshot(test: &PipelineTest) -> Result<()> {
    let src = failed_snapshots_dir_path().join(format!("actual_dump_{}", test.snapshot_name));
    if !src.exists() {
        anyhow::bail!("No actual dump at {}", src.display());
    }
    let dst = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("outputs")
        .join(test.snapshot_name);
    fs::copy(&src, &dst)
        .with_context(|| format!("Failed to copy {} -> {}", src.display(), dst.display()))?;
    info!("Updated {}", dst.display());
    Ok(())
}

fn run_nextest(filters: &[&str]) -> Result<std::process::ExitStatus> {
    let mut cmd = Command::new("cargo");
    scrub_cargo_env(&mut cmd);
    cmd.args([
        "nextest",
        "run",
        "--profile",
        "audit",
        "-p",
        "integration-tests",
        "--no-fail-fast",
    ])
    .args(filters);
    info!("> {cmd:?}");
    let status = cmd.status()?;
    if !status.success() {
        warn!("nextest exited with {status}");
    }
    Ok(status)
}
