#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )"/.. && pwd )"

set +u
if [[ -z "$WORKFLOW_RUN_ID" ]]; then
  echo "WORKFLOW_RUN_ID env variable is required. You can list recent runs using gh run list --workflow \"package for release\" command."
  echo ""
  echo "Recent workflow runs:"
  gh run list --workflow "package for release" | cat
  exit 1
fi

if [[ -z "$RELEASE_TAG" ]]; then
  echo "RELEASE_TAG env variable is required."
  exit 1
fi

if [[ -z "$COMMIT_HASH"  ]]; then
  echo "COMMIT_HASH env variable is required."
  exit 1
fi

if ! docker buildx imagetools >/dev/null 2>&1; then
  echo "Command \"docker buildx imagetools\" failed. Make sure buildx is enabled/installed on your platform."
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "Command \"gh auth status\" failed. Make sure to login authenticate gh CLI."
  exit 1
fi

set -u

mkdir -p "$ROOT_DIR/tools/release_tmp"
cd "$ROOT_DIR/tools/release_tmp"

gh run download "$WORKFLOW_RUN_ID" -n smelter_linux_x86_64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_linux_aarch64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_darwin_x86_64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_darwin_aarch64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_with_web_renderer_linux_x86_64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_with_web_renderer_darwin_x86_64.tar.gz
gh run download "$WORKFLOW_RUN_ID" -n smelter_with_web_renderer_darwin_aarch64.tar.gz

IMAGE_NAME="ghcr.io/software-mansion/smelter"
# Make sure docker is authenticated
# - generate token here https://github.com/settings/tokens
# - Run `docker login ghcr.io -u <username>` to authenticate
docker buildx imagetools create -t "${IMAGE_NAME}:${RELEASE_TAG}-web-renderer" "${IMAGE_NAME}:${COMMIT_HASH}-web-renderer"
docker buildx imagetools create -t "${IMAGE_NAME}:${RELEASE_TAG}" "${IMAGE_NAME}:${COMMIT_HASH}"
docker buildx imagetools create -t "${IMAGE_NAME}:latest" "${IMAGE_NAME}:${COMMIT_HASH}"

gh release create "$RELEASE_TAG"
gh release upload "$RELEASE_TAG" smelter_linux_x86_64.tar.gz
gh release upload "$RELEASE_TAG" smelter_linux_aarch64.tar.gz
gh release upload "$RELEASE_TAG" smelter_darwin_x86_64.tar.gz
gh release upload "$RELEASE_TAG" smelter_darwin_aarch64.tar.gz
gh release upload "$RELEASE_TAG" smelter_with_web_renderer_linux_x86_64.tar.gz
gh release upload "$RELEASE_TAG" smelter_with_web_renderer_darwin_x86_64.tar.gz
gh release upload "$RELEASE_TAG" smelter_with_web_renderer_darwin_aarch64.tar.gz

rm -rf "$ROOT_DIR/tools/release_tmp"
