# `vk-video` release guide

## Required tools
- [`cargo-release`](https://github.com/crate-ci/cargo-release)

## Checklist

- Check if examples work on NVIDIA and AMD
  - Remember to use `--features vk_validation` flag
- Check if `vk-video` compiles on macOS with `--features expose_parsers`
- Check `README.md`
- Check docs
  - Also run: `cargo test --doc`
- Update `CHANGELOG.md`
  - Change current `unreleased` section to `[v{version from Cargo.toml}](LINK TO THE RELEASE/TAG)`
  - Create new `unreleased` section on the top
- Release on crates.io
  - Dry run: `cargo release -p vk-video --tag-prefix "vk-video/"`
  - To actually release add `--execute` flag
- Post on social media
  - Reddit
  - Twitter
  - This Week in Rust
