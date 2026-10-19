#!/usr/bin/env bash
set -euo pipefail
TAG=$(cat "$(git rev-parse --show-toplevel)/.registry-pin")
REPO="https://github.com/christaylor98/axRegistry"
DEST="$(git rev-parse --show-toplevel)/registry-pin"
rm -rf "$DEST"
git clone --branch "$TAG" --depth 1 "$REPO" "$DEST"
echo "Registry pinned at $TAG in $DEST"
