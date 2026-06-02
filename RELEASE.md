# Release

> When releasing both the server and the TypeScript SDK, **release the server first**. `@swmansion/smelter-node` downloads the server binary based on the version pinned in `locallySpawnedInstance.ts`, so the SDK must point at an already-released server tag before it is published.

## Smelter server

### Pre-release verification

Run these checks before releasing a new version:

- Start the demo server (`cargo run --example demo_server`) and exercise every supported input and output type, confirming each works.
- Run the other examples from the `integration-tests` crate.
- Search the changelog for new or changed input/output options. Temporarily modify one of the examples to exercise those new options.

### Release process

To release a new compositor version:

- Go to `Actions`
- Start following actions:
  - [`rust_release_build`](https://github.com/software-mansion/smelter/actions/workflows/rust_release_build.yml)
  - [`docker_publish`](https://github.com/software-mansion/smelter/actions/workflows/docker_publish.yml)
- Wait for a job to finish.
- Run `gh run list --workflow "package for release"` and find an ID of the workflow run that packaged release binaries. Running `./tools/release.sh` without necessary environment variables will also display that list.
- Run

  ```bash
  WORKFLOW_RUN_ID={WORKFLOW_RUN_ID} RELEASE_TAG={VERSION} COMMIT_HASH={COMMIT_HASH} ./tools/release.sh
  ```

  e.g.

  ```bash
  WORKFLOW_RUN_ID=6302155380 RELEASE_TAG=v1.2.3 COMMIT_HASH=8734dd57169ca302d8b19e1def657f78e883a6ca ./tools/release.sh `
  ```

### Versioning

When releasing a new version we create a new branch `v0.[MAJOR]` e.g. `v0.5`. To backport a fix and release a version e.g. `0.5.1`, fix should
be merged to both `master` and `v0.5` branch and released from `v0.5`.

## TypeScript SDK

Packages are published from CI by the [`ts_publish`](https://github.com/software-mansion/smelter/actions/workflows/ts_publish.yml) workflow, which is triggered by pushing a `ts-sdk/v*` tag. It builds and publishes all packages with the `next` dist-tag (already-published versions are skipped).

### Pre-publish verification

Run these checks before bumping versions or pushing the release tag (i.e. before anything is published):

- If this SDK version targets a new server release, update `VERSION` (and `REPO`) in `locallySpawnedInstance.ts` in `@swmansion/smelter-node` to point at it (e.g. `software-mansion/smelter` at `v0.6.0`). That server release must already be published — see the note at the top of this file. Do this first so the checks below test the right binary.
- Test all template projects from `create-smelter-app`. For each template, scaffold a project, build it, and run it to confirm the output works.
- Run all projects in the `ts/examples` directory (they already use the workspace SDK, so there is nothing to update).
- Search the changelog for new or changed input/output options. These are usually mapped one-to-one to fields in the TS SDK, so temporarily modify one of the examples to exercise those same options through the SDK.

### Publishing process

Only bump versions after the verification above passes.

- Run `pnpm bump-version`
  - When releasing first RC version `.pre-release.json` will be created. You need to commit it.
  - When releasing first regular version after RC version, the `.pre-release.json` will be removed. You need to commit that change.
- Commit changes (including the `locallySpawnedInstance.ts` update from verification).
- Push a `ts-sdk/v{VERSION}` tag to trigger the `ts_publish` workflow, which publishes all packages with the `next` dist-tag.
- If you are releasing non-RC version:
  - Retest `create-smelter-app` templates from published `next` version
  - Mark all packages as latest with e.g. `npm dist-tag add @swmansion/smelter@0.3.0 latest`

### Versioning

When releasing a new version of SDK all packages should share a major. After major release we create a branch `ts-sdk/v0.[MAJOR]` e.g `ts-sdk/v0.2`.
To backport a fix and release a version e.g. `0.2.1`, fix should be merged to both `master` and `ts-sdk/v0.2` branch and released from `ts-sdk/v0.2`.

> In particular, if fix that needs to be backported requires changes in SDK and server then the SDK and server changes need to be merged to different branches.
