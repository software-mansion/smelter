use std::{
    fs,
    ops::ControlFlow,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use inquire::{Confirm, InquireError, Select};
use integration_tests::{
    paths::{pipeline_tests_workdir, submodule_root_path, test_workdir},
    pipeline_tests::{PipelineTest, harness::SAVE_DUMPS_ENV, pipeline_tests},
    tools::{rtp_inspector, rtp_player},
};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum Action {
    #[strum(to_string = "Run all pipeline tests")]
    RunAll,
    #[strum(to_string = "Run specific pipeline test")]
    RunSpecific,
    #[strum(to_string = "Audit existing test results (no rerun)")]
    InspectExisting,
    #[strum(to_string = "Restore test_workdir: from GitHub Actions (build_and_test_linux)")]
    DownloadCiArtifacts,
    #[strum(to_string = "Restore test_workdir: from snapshot submodule diff")]
    DiffSnapshotSubmodule,
}

/// Things you can do once a test has produced (or already had on disk)
/// a pair of dumps in the workdir. The dumps don't necessarily come
/// from a failed run — `inspect_existing` or `SMELTER_SAVE_DUMPS=1`
/// also surface dumps for passing tests.
#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum TestResultAction {
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
            Action::InspectExisting => inspect_existing(),
            Action::DownloadCiArtifacts => download_ci_artifacts(),
            Action::DiffSnapshotSubmodule => diff_snapshot_submodule(),
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
    // Single-test runs always preserve dumps so the inspector is
    // available regardless of pass/fail — the user explicitly asked
    // to look at this one.
    let status = run_nextest(&[&filter], RunOptions { save_dumps: true })?;
    if !status.success() {
        let _ = test_result_action_loop(test, &filter)?;
    }
    Ok(())
}

/// Walk every test result already sitting in the work directory and
/// hand each one to the audit UI. Lets the user re-open the inspector
/// after a previous `audit_tests` session, or browse results produced
/// by a separate `cargo nextest` invocation, without having to re-run
/// the (possibly long) test. Inside each iteration the user can
/// `Skip` to advance to the next result; cancelling out of the audit
/// prompt stops the walk.
fn inspect_existing() -> Result<()> {
    let mut tests = discover_tests_with_dumps()?;
    if tests.is_empty() {
        warn!(
            "No test results found in {}",
            pipeline_tests_workdir().display()
        );
        return Ok(());
    }
    tests.sort_by_key(|t| t.full_test_name);
    info!("{} test result(s) to audit", tests.len());
    for test in tests {
        let filter = test_filter(test);
        info!("Auditing test result: {}", test.full_test_name);
        match test_result_action_loop(test, &filter)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => break,
        }
    }
    Ok(())
}

/// Replace `test_workdir/` with the `test_workdir` artifact attached
/// to a chosen CI run. Useful for triaging CI failures locally
/// without having to wait for the test to fail again on this machine.
///
/// Shells out to `gh` (must be installed and authenticated). Listing
/// uses the artifacts API filtered by name (so only runs that
/// actually have a downloadable `test_workdir` artifact appear);
/// downloading uses `gh run download`.
fn download_ci_artifacts() -> Result<()> {
    if !gh_available() {
        anyhow::bail!(
            "`gh` not found in PATH. Install GitHub CLI and `gh auth login` to download CI artifacts."
        );
    }

    let runs = list_recent_runs()?;
    if runs.is_empty() {
        warn!("No CI runs with a `test_workdir` artifact were found");
        return Ok(());
    }

    let labels: Vec<String> = runs.iter().map(ArtifactRun::label).collect();
    let selected_idx = match Select::new("Select a CI run to pull dumps from:", labels)
        .with_page_size(15)
        .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let run = &runs[selected_idx];

    let dest = test_workdir();
    info!(
        "Replacing {} with artifact `test_workdir` from run #{}",
        dest.display(),
        run.run_id
    );

    if !confirm_wipe(
        &dest,
        &format!("Wipe {} and overwrite with CI artifact?", dest.display()),
    )? {
        return Ok(());
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).with_context(|| format!("Failed to clear {}", dest.display()))?;
    }
    fs::create_dir_all(&dest).with_context(|| format!("Failed to create {}", dest.display()))?;

    let mut cmd = Command::new("gh");
    cmd.args([
        "run",
        "download",
        &run.run_id.to_string(),
        "-n",
        "test_workdir",
        "-D",
    ])
    .arg(&dest);
    info!("> {cmd:?}");
    let status = cmd.status().context("Failed to spawn `gh run download`")?;
    if !status.success() {
        anyhow::bail!("`gh run download` exited with {status}");
    }
    info!("Downloaded artifact into {}", dest.display());
    Ok(())
}

/// Populate `pipeline_tests/` workdir from a `git diff` between the
/// snapshot submodule's current working tree (treated as `actual`) and
/// a chosen past commit (treated as `expected`). Only files that
/// changed bit-for-bit are written, so the audit UI lists exactly the
/// snapshots that need a re-look after a snapshot update.
fn diff_snapshot_submodule() -> Result<()> {
    let submodule = submodule_root_path();
    if !submodule.exists() {
        anyhow::bail!(
            "Snapshot submodule not initialized at {}. Run `git submodule update --init --checkout integration-tests/snapshots`.",
            submodule.display()
        );
    }

    // Refresh remote refs first so `origin/main` (and the rest of the
    // picker) reflects the latest upstream state. `--quiet` keeps the
    // output clean; `--tags --prune` makes sure deleted refs disappear.
    info!("Fetching {} ...", submodule.display());
    fetch_origin(&submodule)?;

    let commits = list_snapshot_commits(&submodule)?;
    if commits.is_empty() {
        warn!("No commits found in {}", submodule.display());
        return Ok(());
    }

    let labels: Vec<String> = commits.iter().map(SnapshotCommit::label).collect();
    let selected_idx = match Select::new(
        "Select a past commit to diff current snapshots against:",
        labels,
    )
    .with_page_size(15)
    .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let commit = &commits[selected_idx];

    let dest = pipeline_tests_workdir();
    if !confirm_wipe(
        &dest,
        &format!(
            "Wipe {} and populate from snapshots that differ from {}?",
            dest.display(),
            commit.short_sha()
        ),
    )? {
        return Ok(());
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).with_context(|| format!("Failed to clear {}", dest.display()))?;
    }
    fs::create_dir_all(&dest).with_context(|| format!("Failed to create {}", dest.display()))?;

    let mut written = 0usize;
    let mut skipped_missing_current = 0usize;
    let mut skipped_missing_old = 0usize;
    let mut skipped_identical = 0usize;
    for test in pipeline_tests() {
        let rel = format!("rtp_packet_dumps/outputs/{}", test.snapshot_name);
        let current_path = submodule.join(&rel);
        let current = match fs::read(&current_path) {
            Ok(b) => b,
            Err(_) => {
                skipped_missing_current += 1;
                continue;
            }
        };
        let old = match read_blob_at(&submodule, &commit.sha, &rel) {
            Some(b) => b,
            None => {
                skipped_missing_old += 1;
                continue;
            }
        };
        if current == old {
            skipped_identical += 1;
            continue;
        }
        fs::write(
            dest.join(format!("actual_dump_{}", test.snapshot_name)),
            &current,
        )?;
        fs::write(
            dest.join(format!("expected_dump_{}", test.snapshot_name)),
            &old,
        )?;
        written += 1;
    }

    info!(
        "Wrote {written} differing snapshot(s) into {} \
         (identical: {skipped_identical}, missing current: {skipped_missing_current}, \
         missing at {}: {skipped_missing_old})",
        dest.display(),
        commit.short_sha(),
    );
    Ok(())
}

#[derive(Debug)]
struct SnapshotCommit {
    sha: String,
    date: String,
    subject: String,
    /// When `Some`, the picker shows this in place of the short sha
    /// — used to surface the synthetic `origin/main` entry that
    /// resolves to whatever the remote points at.
    ref_name: Option<String>,
}

impl SnapshotCommit {
    fn short_sha(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }

    fn label(&self) -> String {
        let date = self.date.get(..16).unwrap_or(&self.date);
        let head = match &self.ref_name {
            Some(name) => format!("{name} ({})", self.short_sha()),
            None => self.short_sha().to_string(),
        };
        format!("{head} | {date} | {}", truncate(&self.subject, 70))
    }
}

fn list_snapshot_commits(submodule: &std::path::Path) -> Result<Vec<SnapshotCommit>> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "log",
            "-30",
            "--pretty=format:%H%x09%cI%x09%s",
        ])
        .output()
        .context("Failed to spawn `git log`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`git log` exited with {}: {stderr}", output.status);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let log_commits: Vec<SnapshotCommit> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let sha = parts.next()?.to_string();
            let date = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").to_string();
            Some(SnapshotCommit {
                sha,
                date,
                subject,
                ref_name: None,
            })
        })
        .collect();

    // Build the picker top: the most-frequently-used revisions, in
    // priority order. `git log -30` provides the rest. Pinned entries
    // shadow themselves out of the log to avoid duplicates.
    let mut pinned: Vec<SnapshotCommit> = Vec::new();
    let mut push_pinned = |commit: Option<SnapshotCommit>| {
        if let Some(c) = commit
            && !pinned.iter().any(|p| p.sha == c.sha)
        {
            pinned.push(c);
        }
    };
    push_pinned(resolve_remote_default(submodule));
    push_pinned(resolve_named(submodule, "HEAD"));
    push_pinned(resolve_named(submodule, "HEAD~1"));

    let pinned_shas: std::collections::HashSet<String> =
        pinned.iter().map(|c| c.sha.clone()).collect();
    let mut result = pinned;
    result.extend(log_commits.into_iter().filter(|c| !pinned_shas.contains(&c.sha)));
    Ok(result)
}

/// Try `origin/main` then `origin/master`; return the first that
/// resolves to a commit, with metadata fetched via `git log -1`.
fn resolve_remote_default(submodule: &std::path::Path) -> Option<SnapshotCommit> {
    for name in ["origin/main", "origin/master"] {
        if let Some(commit) = resolve_named(submodule, name) {
            return Some(commit);
        }
    }
    None
}

fn fetch_origin(submodule: &std::path::Path) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "fetch",
            "--quiet",
            "--tags",
            "--prune",
            "origin",
        ])
        .status()
        .context("Failed to spawn `git fetch`")?;
    if !status.success() {
        // A failed fetch is usually transient (offline, auth);
        // surface it as a warning and let the user pick from
        // whatever's already on disk rather than abort the flow.
        warn!(
            "`git fetch origin` exited with {status} — picker will show pre-fetch state of {}",
            submodule.display()
        );
    }
    Ok(())
}

/// Resolve a single named revision (e.g. `HEAD`, `HEAD~1`,
/// `origin/main`) and tag the resulting commit with `name` so the
/// picker shows `name (sha7)` instead of just the short sha.
fn resolve_named(submodule: &std::path::Path, name: &str) -> Option<SnapshotCommit> {
    let commit = log_one(submodule, name)?;
    Some(SnapshotCommit {
        ref_name: Some(name.to_string()),
        ..commit
    })
}

fn log_one(submodule: &std::path::Path, revision: &str) -> Option<SnapshotCommit> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "log",
            "-1",
            "--pretty=format:%H%x09%cI%x09%s",
            revision,
            "--",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let line = line.trim_end_matches('\n');
    let mut parts = line.splitn(3, '\t');
    Some(SnapshotCommit {
        sha: parts.next()?.to_string(),
        date: parts.next()?.to_string(),
        subject: parts.next().unwrap_or("").to_string(),
        ref_name: None,
    })
}

/// Read a single file at `relative_path` as it existed at `sha` in
/// the submodule's history. Returns `None` if the file didn't exist
/// at that revision (or `git show` failed for any other reason).
fn read_blob_at(submodule: &std::path::Path, sha: &str, relative_path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "show",
            &format!("{sha}:{relative_path}"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

/// Confirm overwriting `dir`. Skips the prompt entirely when there's
/// nothing to wipe (dir doesn't exist or is empty). Otherwise asks
/// `message` with a "yes" default — these wipes are launched
/// intentionally, so a single Enter is enough. Cancelling (Esc /
/// Ctrl-C) is treated as "no".
fn confirm_wipe(dir: &std::path::Path, message: &str) -> Result<bool> {
    if !dir_has_content(dir) {
        return Ok(true);
    }
    match Confirm::new(message).with_default(true).prompt() {
        Ok(b) => Ok(b),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn dir_has_content(dir: &std::path::Path) -> bool {
    match fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => false,
    }
}

fn gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// One row of the run picker, derived from a single `test_workdir`
/// artifact entry returned by the GitHub artifacts API. Filtering at
/// the artifact level (rather than listing failed runs and hoping
/// they have an artifact) guarantees every entry is downloadable.
#[derive(Debug)]
struct ArtifactRun {
    run_id: u64,
    head_branch: String,
    head_sha: String,
    created_at: String,
}

impl ArtifactRun {
    fn label(&self) -> String {
        // 'createdAt' is RFC3339; first 16 chars = "YYYY-MM-DDTHH:MM".
        let created = self.created_at.get(..16).unwrap_or(&self.created_at);
        let short_sha = self.head_sha.get(..7).unwrap_or(&self.head_sha);
        format!(
            "{created} | {:<20} | {short_sha} [#{}]",
            truncate(&self.head_branch, 20),
            self.run_id,
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactEntry {
    expired: bool,
    created_at: String,
    workflow_run: ArtifactWorkflowRun,
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactWorkflowRun {
    id: u64,
    head_branch: String,
    head_sha: String,
}

/// Returns the most recent `test_workdir` artifacts attached to runs
/// of this repo, newest first. Hits the artifacts API directly with a
/// name filter so we never surface a run whose artifact doesn't
/// actually exist (or has expired retention).
fn list_recent_runs() -> Result<Vec<ArtifactRun>> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "api",
        "-X",
        "GET",
        "repos/{owner}/{repo}/actions/artifacts",
        "-F",
        "name=test_workdir",
        "-F",
        "per_page=30",
    ]);
    let output = cmd.output().context("Failed to spawn `gh api`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`gh api` exited with {}: {stderr}", output.status);
    }
    let response: ArtifactsResponse =
        serde_json::from_slice(&output.stdout).context("Failed to parse `gh api` output")?;
    // The API already returns newest first; filter expired artifacts
    // last so the list order matches what the user sees on GitHub.
    let runs = response
        .artifacts
        .into_iter()
        .filter(|a| !a.expired)
        .map(|a| ArtifactRun {
            run_id: a.workflow_run.id,
            head_branch: a.workflow_run.head_branch,
            head_sha: a.workflow_run.head_sha,
            created_at: a.created_at,
        })
        .collect();
    Ok(runs)
}

fn run_all() -> Result<()> {
    let workdir = pipeline_tests_workdir();
    if workdir.exists() {
        fs::remove_dir_all(&workdir)
            .with_context(|| format!("Failed to clean {}", workdir.display()))?;
    }

    let status = run_nextest(&["pipeline_tests"], RunOptions::default())?;
    if status.success() {
        return Ok(());
    }

    let tests_with_dumps = discover_tests_with_dumps()?;
    if tests_with_dumps.is_empty() {
        warn!("Test run failed but no test results were left in the workdir");
        return Ok(());
    }

    info!("{} test result(s) to audit", tests_with_dumps.len());
    for test in tests_with_dumps {
        let filter = test_filter(test);
        info!("Auditing test result: {}", test.full_test_name);
        match test_result_action_loop(test, &filter)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => break,
        }
    }
    Ok(())
}

fn discover_tests_with_dumps() -> Result<Vec<&'static PipelineTest>> {
    let workdir = pipeline_tests_workdir();
    let mut found: Vec<&'static PipelineTest> = Vec::new();
    if !workdir.exists() {
        return Ok(found);
    }
    let tests = pipeline_tests();
    for entry in
        fs::read_dir(&workdir).with_context(|| format!("Failed to read {}", workdir.display()))?
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
        if !found.iter().any(|t| std::ptr::eq(*t, test)) {
            found.push(test);
        }
    }
    Ok(found)
}

fn test_filter(test: &PipelineTest) -> String {
    test.full_test_name
        .strip_prefix("integration_tests::")
        .unwrap_or(test.full_test_name)
        .to_string()
}

fn test_result_action_loop(test: &PipelineTest, filter: &str) -> Result<ControlFlow<()>> {
    loop {
        match prompt_test_result_action(test)? {
            Some(TestResultAction::PlayActual) => {
                if let Err(e) = play_dump(test, DumpKind::Actual) {
                    error!("Failed to play actual dump: {e:#}");
                }
            }
            Some(TestResultAction::PlayExpected) => {
                if let Err(e) = play_dump(test, DumpKind::Expected) {
                    error!("Failed to play expected dump: {e:#}");
                }
            }
            Some(TestResultAction::Compare) => {
                if let Err(e) = inspect_dumps(test) {
                    error!("Failed to inspect dumps: {e:#}");
                }
            }
            Some(TestResultAction::UpdateSnapshot) => match update_snapshot(test) {
                Ok(()) => return Ok(ControlFlow::Continue(())),
                Err(e) => error!("Failed to update snapshot: {e:#}"),
            },
            Some(TestResultAction::Rerun) => {
                if let Err(e) = clear_test_dumps(test) {
                    error!("Failed to clear previous test dumps: {e:#}");
                }
                // A rerun targets a single test, so always preserve
                // its dumps for the inspector regardless of pass/fail.
                match run_nextest(&[filter], RunOptions { save_dumps: true }) {
                    Ok(s) if s.success() => {
                        info!("Test passed on rerun. Choose Skip to move on, or rerun again.");
                    }
                    Ok(_) => {}
                    Err(e) => error!("Failed to rerun test: {e:#}"),
                }
            }
            Some(TestResultAction::Skip) => return Ok(ControlFlow::Continue(())),
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

fn prompt_test_result_action(test: &PipelineTest) -> Result<Option<TestResultAction>> {
    use strum::IntoEnumIterator;
    const BOLD: &str = "\x1b[1m";
    const CYAN: &str = "\x1b[36m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    println!();
    println!("{BOLD}{YELLOW}── Test result ──────────────────────────────────────{RESET}");
    println!("{BOLD}{CYAN}{}{RESET}", test.full_test_name);
    if !test.description.is_empty() {
        println!("{}", test.description);
    }
    println!("{BOLD}{YELLOW}─────────────────────────────────────────────────────{RESET}");
    println!();

    let actual_exists = pipeline_tests_workdir()
        .join(format!("actual_dump_{}", test.snapshot_name))
        .exists();
    let expected_exists = pipeline_tests_workdir()
        .join(format!("expected_dump_{}", test.snapshot_name))
        .exists();
    let options: Vec<TestResultAction> = TestResultAction::iter()
        .filter(|a| match a {
            TestResultAction::UpdateSnapshot => actual_exists,
            // Inspector tolerates either side missing now, so as long
            // as we have anything at all to look at, offer it.
            TestResultAction::Compare => actual_exists || expected_exists,
            TestResultAction::PlayExpected => expected_exists,
            TestResultAction::PlayActual => actual_exists,
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

fn clear_test_dumps(test: &PipelineTest) -> Result<()> {
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
    let path =
        pipeline_tests_workdir().join(format!("{}{}", kind.file_prefix(), test.snapshot_name));
    if !path.exists() {
        warn!("Dump not found: {}", path.display());
        return Ok(());
    }
    info!("Press Esc or q to stop playback");
    run_with_kill_on_key(rtp_player::spawn(&path)?)
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
    let dir = pipeline_tests_workdir();
    let actual = dir.join(format!("actual_dump_{}", test.snapshot_name));
    let expected = dir.join(format!("expected_dump_{}", test.snapshot_name));

    rtp_inspector::run(&expected, &actual)
}

fn update_snapshot(test: &PipelineTest) -> Result<()> {
    let src = pipeline_tests_workdir().join(format!("actual_dump_{}", test.snapshot_name));
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

#[derive(Debug, Clone, Copy, Default)]
struct RunOptions {
    /// Set [`SAVE_DUMPS_ENV`] in the child process so the harness
    /// always writes both expected/actual dumps to the workdir, even
    /// for tests that pass.
    save_dumps: bool,
}

fn run_nextest(filters: &[&str], options: RunOptions) -> Result<std::process::ExitStatus> {
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
    if options.save_dumps {
        cmd.env(SAVE_DUMPS_ENV, "1");
    }
    info!("> {cmd:?}");
    let status = cmd.status()?;
    if !status.success() {
        warn!("nextest exited with {status}");
    }
    Ok(status)
}
