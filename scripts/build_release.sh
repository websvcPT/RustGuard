#!/usr/bin/env bash
set -euo pipefail

# Unified release build entrypoint used by CI and local packaging flows.
# Usage: ./scripts/build_release.sh <version> <linux|windows|all>

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_MANIFEST="$ROOT_DIR/app/Cargo.toml"

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <version> <linux|windows|all>" >&2
  exit 1
fi

VERSION="${1#v}"
TARGET="$2"

if [[ -z "$VERSION" ]]; then
  echo "Version cannot be empty" >&2
  exit 1
fi

set_manifest_version() {
  local new_version="$1"
  sed -i -E "0,/^version = \".*\"/s//version = \"$new_version\"/" "$APP_MANIFEST"
}

build_linux() {
  RUSTGUARD_MANIFEST_ALREADY_PATCHED=1 "$ROOT_DIR/scripts/build_linux.sh" "$VERSION"
}

build_windows() {
  RUSTGUARD_MANIFEST_ALREADY_PATCHED=1 "$ROOT_DIR/scripts/build_windows.sh" "$VERSION"
}

case "$TARGET" in
  linux)
    set_manifest_version "$VERSION"
    trap 'set_manifest_version "0.0.0"' EXIT
    build_linux
    ;;
  windows)
    set_manifest_version "$VERSION"
    trap 'set_manifest_version "0.0.0"' EXIT
    build_windows
    ;;
  all)
    set_manifest_version "$VERSION"
    trap 'set_manifest_version "0.0.0"' EXIT
    build_linux &
    linux_pid=$!
    build_windows &
    windows_pid=$!

    wait "$linux_pid"
    wait "$windows_pid"
    ;;
  *)
    echo "Unknown target '$TARGET' (expected: linux, windows, all)" >&2
    exit 1
    ;;
esac
