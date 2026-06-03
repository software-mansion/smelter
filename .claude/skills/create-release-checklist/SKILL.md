---
name: create-release-checklist
description: Generate a GitHub release-checklist issue for a full (non-RC) release of the Smelter server and/or the TypeScript SDK. Reads the changelog, extracts the actual new/changed options to verify, and opens the issue with gh.
disable-model-invocation: false
allowed-tools: Bash, Read, Grep, Glob
---

Create a GitHub issue that tracks a **full, non-RC release** of the Smelter server and the TypeScript SDK, following the structure and ordering of the process documented in `RELEASE.md`.

This skill is for **non-RC releases only**. Do not add any "if non-RC" hedging, RC tags, or `.pre-release.json`-creation steps — assume the release promotes to `latest`.

## What to gather first

1. **Current versions and last released tags** (so you can propose the next versions):
   - Server: `version` in the workspace `Cargo.toml`; last released `git tag --list 'v0.*'`.
   - SDK: `version` in `ts/smelter/package.json` and `ts/smelter-node/package.json`; last released `git tag --list 'ts-sdk/*'`.
2. **The changelog**: read the `## unreleased` section of `CHANGELOG.md`. This is the source of truth for what needs to be verified — do not invent or generalize it.
3. **The server↔SDK coupling**: `ts/smelter-node/src/manager/locallySpawnedInstance.ts` defines `VERSION` and `REPO`. The SDK downloads the server binary from there, so the SDK release depends on the server release.

Propose the next versions to the user and confirm before creating the issue:
- Server uses a `0.MAJOR.MINOR` scheme. If the `## unreleased` section has a `💥 Breaking changes` block, bump the major segment (e.g. `0.5.0` → `0.6.0`); otherwise bump the patch.
- All SDK packages share a major; pick the matching SDK version.

Do **not** add a "Decide versions" checklist item to the issue itself — bake the chosen versions directly into the title and steps.

## Extracting options from the changelog

Go through every entry under `## unreleased` (`💥 Breaking changes`, `✨ New features`, plus any input/output-relevant `🔧 Others`) and turn each new or changed **input / output / encoder option, endpoint, WebSocket event, or shader requirement** into one leaf checklist item. Quote the actual field/option/env-var names from the changelog.

Classify each one:
- **Maps to a TS SDK field** (input/output registration options, encoder options, input-update operations, events, shader requirements) → goes in **both** Phase A (server) and Phase B (SDK). The server options map ~one-to-one to TS SDK fields.
- **Server-config only** (anything configured via an env var, e.g. `SMELTER_RTMP_SERVER_PORT`, the `SMELTER_WEBRTC_*` ICE vars) → goes in Phase A only. In Phase B, list these once as a single note saying they are server-config, not SDK fields, and were verified in Phase A.

## Issue structure (reproduce exactly)

These conventions come from real corrections — keep them:

- **Checkboxes on leaf tasks only.** Structural/grouping lines (phase headers, `A1.`/`B2.` step groups, the "Temporarily modify examples…" lead-in) are plain text. Only the smallest actionable items get `- [ ]`.
- **Verification before version bump, always.** In each phase the verification step comes first; the version-bump PR step is explicitly gated "only after verification passes". Never bump versions before testing.
- **Server first; SDK depends on it.** Phase A is the server and is labeled as release-first. Phase B is gated on the server being published, and its first step points `locallySpawnedInstance.ts` at the freshly released server tag — this is done *before* SDK verification so the `create-smelter-app` templates test the right binary. It is not a version bump.
- **Ordering/dependency warnings are inline ordered points, not global blockquotes.** Put them in the step where they apply (a leading `⚠️` is fine). Do not stack notes at the top of the issue.
- **No obvious/self-evident items.** Skip filler like "discard temporary example edits" or "decide versions" — only list real, non-trivial actions.

Use this body as the template (substitute the real versions and the changelog-derived option lines):

```markdown
Release checklist for **Smelter server `vX.Y.Z`** and **TypeScript SDK `A.B.C`**.

**Phase A — Smelter server** (release this first — Phase B depends on it)

- A1. Pre-release verification (on `master`, before any version bump)
  - [ ] Start demo server (`cargo run --example demo_server` + `cargo run --example demo`) and exercise every input and output type
  - [ ] Run the other examples in the `integration-tests` crate
  - ⚠️ Temporarily modify examples to test the new/changed options below (extracted from the `## unreleased` section of `CHANGELOG.md`):
    - [ ] <one leaf per new/changed option, quoting the real names>
    - [ ] ...
- A2. 📌 Create release PR (only after verification passes)
  - [ ] Bump `version` in workspace `Cargo.toml` (`<old>` → `<new>`)
  - [ ] In `CHANGELOG.md`, rename `## unreleased` → `## [vX.Y.Z](…/releases/tag/vX.Y.Z)` and add a fresh empty `## unreleased`
  - [ ] Review + merge to `master`; create `v0.Y` branch per versioning policy
- A3. Release process (after PR merged)
  - [ ] Trigger `rust_release_build` and `docker_publish` GitHub Actions
  - [ ] `gh run list --workflow "package for release"` → get `WORKFLOW_RUN_ID`
  - [ ] Run `WORKFLOW_RUN_ID={ID} RELEASE_TAG=vX.Y.Z COMMIT_HASH={merged commit} ./tools/release.sh`

**Phase B — TypeScript SDK** (only after the server from Phase A is released and published)

- B1. Point the SDK at the released server (needed so verification tests the right binary, not a version bump)
  - [ ] ⚠️ `@swmansion/smelter-node` downloads the server binary, so update `ts/smelter-node/src/manager/locallySpawnedInstance.ts` to the freshly released tag: `VERSION = 'vX.Y.Z'`, `REPO = 'software-mansion/smelter'` (remove the RC-repo lines)
- B2. Pre-publish verification (against the workspace SDK, before bumping versions)
  - [ ] Test every `create-smelter-app` template (scaffold, build, run — confirms the new binary URL resolves)
  - [ ] Run all `ts/examples` projects (already use the workspace SDK)
  - The changelog options map ~one-to-one to TS SDK fields — modify an example to exercise the new/changed ones through the SDK:
    - [ ] <the SDK-field-mapping options, mirroring Phase A>
    - [ ] (server-config only, not SDK fields) <env-var options> — verified in Phase A
- B3. 📌 Create version-bump PR (only after verification passes)
  - [ ] Run `pnpm bump-version` (sets package versions to `A.B.C` and removes `.pre-release.json` — commit that removal)
  - [ ] Commit (including the `locallySpawnedInstance.ts` change from B1)
  - [ ] Review + merge to `master`; create `ts-sdk/v0.B` branch
- B4. Publishing process
  - [ ] Push the `ts-sdk/vA.B.C` tag → triggers `ts_publish` workflow (publishes all packages with `next` dist-tag)
  - [ ] Re-test `create-smelter-app` templates against the published `next` packages
  - [ ] Promote to latest — `npm dist-tag add @swmansion/smelter@A.B.C latest` (and each other package)
```

## Creating the issue

Write the rendered body to a temp file and create the issue:

```bash
gh issue create --repo software-mansion/smelter \
  --title "Release: Smelter server vX.Y.Z + TypeScript SDK A.B.C" \
  --body-file /tmp/release-issue.md
```

Report the created issue URL back to the user.

If the user is releasing only the server or only the SDK, include just that phase, and drop the server↔SDK ordering/`locallySpawnedInstance.ts` coupling (it only matters when both are released together).
