# Release

## Smelter server

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

## TypeScript SDK

Publishing packages is not automated. Follow this steps when releasing a new version.

- Run `pnpm bump-types`
  - When releasing first RC version `.pre-release.json` will be created. You need to commit it.
  - When releasing first regular version after RC version, the `.pre-release.json` will be removed. You need to commit that change.
- Update url in `locallySpawnedInstance.ts` in `@swmansion/smelter-node`
- Commit changes
- Run `pnpm publish --tag next` in all packages in this order:
  - `@swmansion/smelter`
  - `@swmansion/smelter-core`
  - `@swmansion/smelter-node`
  - `@swmansion/smelter-web-client`
  - `@swmansion/smelter-browser-render`
  - `@swmansion/smelter-web-wasm`
  - `create-smelter-app`
- Test if everything works:
  - Update projects in `/demos` directory.
  - ...
- Mark all packages as latest with e.g. `npm dist-tag add @swmansion/smelter@0.3.0 latest`

## Versioning

### Smelter server

When releasing a new version we create a new branch `v0.[MAJOR]` e.g. `v0.5`. To backport a fix and release a version e.g. `0.5.1`, fix should
be merged to both `master` and `v0.5` branch and released from `v0.5`.

### TypeScript SDK

When releasing a new version of SDK all packages should share a major. After major release we create a branch `ts-sdk/v0.[MAJOR]` e.g `ts-sdk/v0.2`.
To backport a fix and release a version e.g. `0.2.1`, fix should be merged to both `master` and `ts-sdk/v0.2` branch and released from `ts-sdk/v0.2`.

> In particular, if fix that needs to be backported requires changes in SDK and server then the SDK and server changes need to be merged to different branches.

