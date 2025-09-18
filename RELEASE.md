# Release

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
