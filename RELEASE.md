# Release

## Smelter server

To release a new compositor version:

- Go to `Actions` -> [`package for release`](https://github.com/software-mansion/smelter/actions/workflows/package_for_release.yml) -> Trigger build on master using "Run workflow" drop-down menu.
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

## TypeScript SDK

Publishing packages is not automated, when releasing you need to update versions across the repository and run `pnpm publish` in
packages you want to publish in the appropriate order

## Versioning

### Smelter server

When releasing a new version we create a new branch `v0.[MAJOR]` e.g. `v0.5`. To backport a fix and release a version e.g. `0.5.1`, fix should
be merged to both `master` and `v0.5` branch and released from `v0.5`.

### TypeScript SDK

When releasing a new version of SDK all packages should share a major. After major release we create a branch `ts-sdk/v0.[MAJOR]` e.g `ts-sdk/v0.2`.
To backport a fix and release a version e.g. `0.2.1`, fix should be merged to both `master` and `ts-sdk/v0.2` branch and released from `ts-sdk/v0.2`.

> In particular, if fix that needs to be backported requires changes in SDK and server then the SDK and server changes need to be merged to different branches.

