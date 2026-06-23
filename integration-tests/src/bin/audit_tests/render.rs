use std::{fs, ops::ControlFlow, path::PathBuf};

use anyhow::{Context, Result};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use inquire::{InquireError, Select};
use integration_tests::{
    paths::{render_snapshots_dir_path, render_tests_workdir},
    render_tests::{RenderTest, render_tests},
};
use tracing::{error, info, warn};

use crate::{RunOptions, render_tests_inspector, run_test, walk_dir};

pub(crate) fn run_all_render() -> Result<()> {
    let workdir = render_tests_workdir();
    if workdir.exists() {
        fs::remove_dir_all(&workdir)
            .with_context(|| format!("Failed to clean {}", workdir.display()))?;
    }

    let status = run_test("test(/render_tests/)", RunOptions::default())?;
    if status.success() {
        return Ok(());
    }

    let mut tests = discover_render_tests_with_snapshots()?;
    if tests.is_empty() {
        warn!("Render tests failed but no snapshot pairs were left in the workdir");
        return Ok(());
    }
    tests.sort_by_key(|t| t.full_test_name);
    info!("{} failed render test(s) to audit", tests.len());
    let _ = audit_render_tests(&tests)?;
    Ok(())
}

pub(crate) fn run_specific_render() -> Result<()> {
    let mut tests: Vec<&'static RenderTest> = render_tests();
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
    let selected_idx = match Select::new("Select a render test:", labels)
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
    let workdir = render_tests_workdir();
    if workdir.exists() {
        fs::remove_dir_all(&workdir)
            .with_context(|| format!("Failed to clean {}", workdir.display()))?;
    }
    let filter = render_test_filter(test);
    // Single-test runs always preserve dumps so the audit menu is
    // available regardless of pass/fail — the user explicitly asked
    // to look at this one.
    let _status = run_test(&filter, RunOptions { save_dumps: true })?;

    let mut inspector: Option<render_tests_inspector::InspectorHandle> = None;
    let _ = audit_render_test(test, &mut inspector)?;
    Ok(())
}

pub(crate) fn audit_existing_render() -> Result<()> {
    let mut tests = discover_render_tests_with_snapshots()?;
    if tests.is_empty() {
        warn!("No render test results found in {}", render_tests_workdir().display());
        return Ok(());
    }
    tests.sort_by_key(|t| t.full_test_name);
    info!("{} render test(s) to audit", tests.len());
    if let ControlFlow::Break(()) = audit_render_tests(&tests)? {
        return Ok(());
    }
    Ok(())
}

/// Drive the per-test audit UI across a list of tests. The inspector
/// is kept alive across tests so the same window is reused.
fn audit_render_tests(tests: &[&'static RenderTest]) -> Result<ControlFlow<()>> {
    let mut inspector: Option<render_tests_inspector::InspectorHandle> = None;
    for test in tests {
        info!("Auditing render test: {}", test.full_test_name);
        match audit_render_test(test, &mut inspector)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => return Ok(ControlFlow::Break(())),
        }
    }
    Ok(ControlFlow::Continue(()))
}

/// Per-test loop with a single flat menu. Each loop iteration refreshes
/// the snapshot pair list from disk — a `Rerun` can add or drop PTSs.
/// The menu lists each PTS alongside `Update`, `Rerun`, `Next test`.
/// Picking a PTS opens (or refreshes) the inspector and tracks that PTS
/// as the "currently opened" one; `Update snapshot` then operates on
/// whichever PTS was last opened.
fn audit_render_test(
    test: &RenderTest,
    inspector: &mut Option<render_tests_inspector::InspectorHandle>,
) -> Result<ControlFlow<()>> {
    let mut opened_snapshot_name: Option<String> = None;
    let mut cursor: usize = 0;
    loop {
        let pairs = render_snapshot_pairs_for_test(test)?;
        // Forget the opened pts if a rerun dropped it; otherwise the
        // Update label would point at a snapshot that no longer exists.
        if opened_snapshot_name
            .as_deref()
            .is_some_and(|name| !pairs.iter().any(|p| p.snapshot_name == name))
        {
            opened_snapshot_name = None;
        }

        let selection = prompt_render_test_menu(
            test,
            &pairs,
            opened_snapshot_name.as_deref(),
            cursor,
        )?;
        let (choice, picked_idx) = match selection {
            None => return Ok(ControlFlow::Break(())),
            Some(pair) => pair,
        };
        cursor = picked_idx;
        match choice {
            RenderTestMenuChoice::NextTest => return Ok(ControlFlow::Continue(())),
            RenderTestMenuChoice::RerunTest => {
                rerun_render_test(test);
                // Inspector reads PNGs at open/refresh time and doesn't
                // watch the filesystem, so after the rerun overwrites
                // the workdir we have to push a refresh ourselves.
                if let (Some(name), Some(insp)) =
                    (opened_snapshot_name.as_deref(), inspector.as_ref())
                {
                    let workdir = render_tests_workdir();
                    let expected = workdir.join(format!("expected_{name}"));
                    let actual = workdir.join(format!("actual_{name}"));
                    if !render_tests_inspector::refresh(insp, &expected, &actual) {
                        *inspector = None;
                    }
                }
            }
            RenderTestMenuChoice::OpenPts(idx) => {
                let snapshot = &pairs[idx];
                opened_snapshot_name = Some(snapshot.snapshot_name.clone());
                let alive = inspector.as_ref().is_some_and(|insp| {
                    render_tests_inspector::refresh(
                        insp,
                        &snapshot.expected,
                        &snapshot.actual,
                    )
                });
                if !alive {
                    match render_tests_inspector::open(
                        &snapshot.expected,
                        &snapshot.actual,
                    ) {
                        Ok(insp) => *inspector = Some(insp),
                        Err(e) => error!("Failed to launch render inspector: {e:#}"),
                    }
                }
            }
            RenderTestMenuChoice::UpdateSnapshot => {
                let Some(name) = opened_snapshot_name.as_deref() else {
                    warn!("Open a snapshot first before updating");
                    continue;
                };
                let Some(snapshot) = pairs.iter().find(|p| p.snapshot_name == name)
                else {
                    warn!("Opened snapshot {name} is no longer in the workdir");
                    opened_snapshot_name = None;
                    continue;
                };
                if let Err(e) = update_render_snapshot(snapshot) {
                    error!("Failed to update snapshot: {e:#}");
                }
            }
        }
    }
}

#[derive(Debug)]
enum RenderTestMenuChoice {
    /// Index into the `pairs` slice that produced the menu.
    OpenPts(usize),
    UpdateSnapshot,
    RerunTest,
    NextTest,
}

fn prompt_render_test_menu(
    test: &RenderTest,
    pairs: &[RenderSnapshotPair],
    opened_snapshot_name: Option<&str>,
    starting_cursor: usize,
) -> Result<Option<(RenderTestMenuChoice, usize)>> {
    const BOLD: &str = "\x1b[1m";
    const CYAN: &str = "\x1b[36m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    println!();
    println!(
        "{BOLD}{YELLOW}── Render test ──────────────────────────────────────{RESET}"
    );
    println!("{BOLD}{CYAN}{}{RESET}", test.full_test_name);
    if !test.description.is_empty() {
        println!("{}", test.description);
    }
    println!(
        "{BOLD}{YELLOW}─────────────────────────────────────────────────────{RESET}"
    );
    println!();

    // When a snapshot is open, surface `Update` first — the common
    // flow is open → review → update, so the user can hit Enter on
    // the next iteration to commit what they just looked at.
    let mut labels: Vec<String> = Vec::new();
    let update_idx = opened_snapshot_name.map(|name| {
        labels.push(format!("Update snapshot from actual ({name})"));
        labels.len() - 1
    });
    for p in pairs {
        let marker = if Some(p.snapshot_name.as_str()) == opened_snapshot_name {
            " [open]"
        } else {
            ""
        };
        labels.push(format!("Open snapshot {}{}", p.snapshot_name, marker));
    }
    let rerun_idx = labels.len();
    labels.push("Rerun test".to_string());
    let next_idx = labels.len();
    labels.push("Next test".to_string());

    // `with_starting_cursor` panics on out-of-bounds — a rerun can
    // shrink the list, so clamp first.
    let starting_cursor = starting_cursor.min(labels.len().saturating_sub(1));

    match Select::new("What next?", labels)
        .with_page_size(15)
        .with_starting_cursor(starting_cursor)
        .raw_prompt()
    {
        Ok(s) => {
            let idx = s.index;
            // PTS rows start right after the (optional) Update slot,
            // so translate the menu index back into a `pairs` index.
            let pts_start = update_idx.map(|i| i + 1).unwrap_or(0);
            let choice = if Some(idx) == update_idx {
                RenderTestMenuChoice::UpdateSnapshot
            } else if idx == rerun_idx {
                RenderTestMenuChoice::RerunTest
            } else if idx == next_idx {
                RenderTestMenuChoice::NextTest
            } else {
                RenderTestMenuChoice::OpenPts(idx - pts_start)
            };
            Ok(Some((choice, idx)))
        }
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

/// Re-run a single render test from scratch: clear every snapshot
/// pair already on disk for it (so stale PTSs from the previous run
/// don't linger) and shell out to nextest. The next iteration of the
/// test menu will re-read whatever the rerun produced.
fn rerun_render_test(test: &RenderTest) {
    match render_snapshot_pairs_for_test(test) {
        Ok(pairs) => {
            for snapshot in &pairs {
                if let Err(e) = clear_render_snapshot_pair(snapshot) {
                    error!("Failed to clear previous snapshot files: {e:#}");
                }
            }
        }
        Err(e) => error!("Failed to enumerate snapshot pairs before rerun: {e:#}"),
    }
    let filter = render_test_filter(test);
    match run_test(&filter, RunOptions { save_dumps: true }) {
        Ok(s) if s.success() => {
            info!("Test passed on rerun.");
        }
        Ok(_) => {}
        Err(e) => error!("Failed to rerun test: {e:#}"),
    }
}

/// One render-test snapshot pair found in the workdir. The two paths
/// are kept separately because either side may be missing — the
/// inspector can still open with a placeholder when, for example,
/// this is a brand-new test with no committed `expected`.
#[derive(Debug, Clone)]
struct RenderSnapshotPair {
    /// File name (without `actual_` / `expected_` prefix), e.g.
    /// `simple__simple_input_pass_through_00000_output_1.png`.
    snapshot_name: String,
    /// `<workdir>/actual_<snapshot_name>`.
    actual: PathBuf,
    /// `<workdir>/expected_<snapshot_name>`.
    expected: PathBuf,
}

fn render_test_filter(test: &RenderTest) -> String {
    let name = test
        .full_test_name
        .strip_prefix("integration_tests::")
        .unwrap_or(test.full_test_name);
    format!("test(={name})")
}

/// Walk the render-test workdir, match each `actual_*.png` /
/// `expected_*.png` back to a registered `RenderTest`, and return one
/// entry per unique test that has any snapshot files on disk.
fn discover_render_tests_with_snapshots() -> Result<Vec<&'static RenderTest>> {
    let workdir = render_tests_workdir();
    let mut found: Vec<&'static RenderTest> = Vec::new();
    if !workdir.exists() {
        return Ok(found);
    }
    for entry in fs::read_dir(&workdir)
        .with_context(|| format!("Failed to read {}", workdir.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.ends_with(".png") {
            continue;
        }
        let Some(snapshot) =
            name.strip_prefix("actual_").or_else(|| name.strip_prefix("expected_"))
        else {
            continue;
        };
        let Some(test) = match_render_test_for_workdir_snapshot(snapshot) else {
            warn!("No registered RenderTest matches snapshot {snapshot}");
            continue;
        };
        if !found.iter().any(|t| std::ptr::eq(*t, test)) {
            found.push(test);
        }
    }
    Ok(found)
}

/// Collect all `actual_*` / `expected_*` snapshot pairs in the
/// render-test workdir that belong to a single `RenderTest`. A test
/// can produce multiple pairs (one per rendered PTS).
fn render_snapshot_pairs_for_test(test: &RenderTest) -> Result<Vec<RenderSnapshotPair>> {
    let workdir = render_tests_workdir();
    if !workdir.exists() {
        return Ok(Vec::new());
    }
    let mut by_name: std::collections::BTreeMap<String, (bool, bool)> =
        Default::default();
    for entry in fs::read_dir(&workdir)
        .with_context(|| format!("Failed to read {}", workdir.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.ends_with(".png") {
            continue;
        }
        let (rest, is_actual) = if let Some(r) = name.strip_prefix("actual_") {
            (r, true)
        } else if let Some(r) = name.strip_prefix("expected_") {
            (r, false)
        } else {
            continue;
        };
        if match_render_test_for_workdir_snapshot(rest)
            .is_none_or(|t| !std::ptr::eq(t, test))
        {
            continue;
        }
        let slot = by_name.entry(rest.to_string()).or_default();
        if is_actual {
            slot.0 = true;
        } else {
            slot.1 = true;
        }
    }
    Ok(by_name
        .into_keys()
        .map(|snapshot_name| {
            let actual = workdir.join(format!("actual_{snapshot_name}"));
            let expected = workdir.join(format!("expected_{snapshot_name}"));
            RenderSnapshotPair { snapshot_name, actual, expected }
        })
        .collect())
}

/// Copy `actual_<name>.png` from the workdir over its committed
/// counterpart in `render_snapshots/`. The destination is the
/// canonical `<test.module>/<name>` derived from the matching
/// `RenderTest`.
fn update_render_snapshot(snapshot: &RenderSnapshotPair) -> Result<()> {
    if !snapshot.actual.exists() {
        anyhow::bail!("No actual snapshot at {}", snapshot.actual.display());
    }
    let dst = committed_snapshot_path(&snapshot.snapshot_name)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::copy(&snapshot.actual, &dst).with_context(|| {
        format!("Failed to copy {} -> {}", snapshot.actual.display(), dst.display())
    })?;
    info!("Updated {}", dst.display());
    Ok(())
}

/// Translate a workdir snapshot name (`<module>__<test_name>_<pts>_output_<n>.png`)
/// to the committed location the snapshot harness reads from:
/// `render_snapshots/<module>/<test_name>_<pts>_output_<n>.png`.
fn committed_snapshot_path(workdir_snapshot_name: &str) -> Result<PathBuf> {
    let test = match_render_test_for_workdir_snapshot(workdir_snapshot_name).with_context(|| {
        format!(
            "Cannot derive destination for snapshot {workdir_snapshot_name}: no matching RenderTest"
        )
    })?;
    let committed_name = workdir_snapshot_name
        .strip_prefix(&format!("{}__", test.module))
        .with_context(|| {
            format!("workdir snapshot name `{workdir_snapshot_name}` missing `<module>__` prefix")
        })?;
    Ok(render_snapshots_dir_path().join(test.module).join(committed_name))
}

fn clear_render_snapshot_pair(snapshot: &RenderSnapshotPair) -> Result<()> {
    for path in [&snapshot.actual, &snapshot.expected] {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

/// Parse a workdir snapshot file name (the part after `actual_` /
/// `expected_`). Workdir naming: `<module>__<test_name>_<pts:05>_output_<n>.png`.
/// Returns `(module, test_name)`.
fn parse_workdir_snapshot_name(name: &str) -> Option<(&str, &str)> {
    let stem = name.strip_suffix(".png")?;
    // Drop trailing `_output_<n>` first, then the `_<pts:05>` segment.
    let (head, _) = stem.rsplit_once("_output_")?;
    let (module_and_name, _pts) = head.rsplit_once('_')?;
    module_and_name.split_once("__")
}

/// Look up the `RenderTest` for a workdir snapshot file name. Match
/// on both `module` and `test_name` — same `test_name` in different
/// modules would otherwise alias.
fn match_render_test_for_workdir_snapshot(name: &str) -> Option<&'static RenderTest> {
    let (module, test_name) = parse_workdir_snapshot_name(name)?;
    render_tests().into_iter().find(|t| t.module == module && t.test_name == test_name)
}

/// Look up the `RenderTest` for a committed snapshot file. The
/// committed layout encodes module as the parent directory and the
/// file name is `<test_name>_<pts:05>_output_<n>.png`; we match on
/// both.
fn match_render_test_for_committed_snapshot(
    file_name: &str,
    parent_dir: &str,
) -> Option<&'static RenderTest> {
    let stem = file_name.strip_suffix(".png")?;
    let (head, _) = stem.rsplit_once("_output_")?;
    let (test_name, _pts) = head.rsplit_once('_')?;
    render_tests()
        .into_iter()
        .find(|t| t.module == parent_dir && t.test_name == test_name)
}

pub(crate) fn find_orphan_render_snapshots() -> Result<Vec<PathBuf>> {
    let root = render_snapshots_dir_path();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut orphans: Vec<PathBuf> = Vec::new();
    walk_dir(&root, &mut |path| {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        if !name.ends_with(".png") {
            return;
        }
        // The snapshot harness writes to `<test.module>/<test.test_name>_...`,
        // so a file is only "used" when both its name and parent
        // directory line up with the same registered test.
        let Some(parent_dir) = path
            .parent()
            .and_then(|p| p.strip_prefix(&root).ok())
            .and_then(|p| p.to_str())
        else {
            return;
        };
        if match_render_test_for_committed_snapshot(name, parent_dir).is_none() {
            orphans.push(path.to_path_buf());
        }
    })?;
    orphans.sort();
    Ok(orphans)
}
