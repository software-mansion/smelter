#!/usr/bin/env bash
set -euo pipefail

BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" = "HEAD" ]; then
  echo "Error: not on a branch (detached HEAD)"
  exit 1
fi
if [ "$BRANCH" = "master" ]; then
  echo "Error: refusing to deploy from master branch"
  exit 1
fi

git push
ssh puffer.fishjam.io "cd /root/smelter-test && git fetch && git checkout -f $BRANCH && git reset --hard origin/$BRANCH && docker compose -f ts/examples/sandbox/compose.yml up -d --build"
