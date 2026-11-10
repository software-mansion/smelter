# `gpu-video` release guide

## Required tools
- [`cargo-release`](https://github.com/crate-ci/cargo-release)

## Checklist

- Check if examples work on Linux, macOS and Windows (test on NVIDIA and AMD)
- Check `README.md` and `CHANGELOG.md`
- Check docs
  - Also run: `cargo test --doc`
- Update `CHANGELOG.md`
  - Change current `unreleased` section to `[v{version from Cargo.toml}](LINK TO THE RELEASE/TAG)`
  - Create new `unreleased` section on the top
- Release on crates.io
  - Dry run: `cargo release -p gpu-video --tag-prefix "gpu-video/"`
  - To actually release add `--execute` flag
