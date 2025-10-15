#!/usr/bin/env bash

set -euo pipefail

cargo build -r -p integration-tests --example simpler

cargo run -p tools --bin package_for_release

mkdir -p ~/smelter-test/

cp ./tools/build/smelter/dependency_check ~/smelter-test/
cp -r ./tools/build/smelter/smelter.app/ ~/smelter-test/
cp ./target/release/examples/simpler ~/smelter-test/

