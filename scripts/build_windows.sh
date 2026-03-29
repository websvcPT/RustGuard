#!/usr/bin/env bash
set -euo pipefail

# Repository root used as canonical base for relative paths.
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
# Cargo manifest patched during versioned release builds.
APP_MANIFEST="$ROOT_DIR/app/Cargo.toml"
# Output directory for Windows artifacts consumed by release workflow.
DIST_DIR="$ROOT_DIR/dist/windows"

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  if [[ "$CARGO_TARGET_DIR" = /* ]]; then
    TARGET_DIR="$CARGO_TARGET_DIR"
  else
    TARGET_DIR="$ROOT_DIR/$CARGO_TARGET_DIR"
  fi
else
  TARGET_DIR="$ROOT_DIR/target"
fi
export CARGO_TARGET_DIR="$TARGET_DIR"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <version>" >&2
  exit 1
fi

VERSION="${1#v}"
if [[ -z "$VERSION" ]]; then
  echo "Version cannot be empty" >&2
  exit 1
fi

set_manifest_version() {
  local new_version="$1"
  sed -i -E "0,/^version = \".*\"/s//version = \"$new_version\"/" "$APP_MANIFEST"
}

rm -rf "$DIST_DIR"
if [[ "${RUSTGUARD_MANIFEST_ALREADY_PATCHED:-0}" == "1" ]]; then
  trap ':' EXIT
else
  trap 'set_manifest_version "0.0.0"' EXIT
  set_manifest_version "$VERSION"
fi

# Use native build on Windows hosts and cross-build on non-Windows hosts.
if [[ "${OS:-}" == "Windows_NT" ]]; then
  cargo build --release --manifest-path "$APP_MANIFEST"
  BIN="$TARGET_DIR/release/rustguard.exe"
else
  cargo build --release --target x86_64-pc-windows-gnu --manifest-path "$APP_MANIFEST"
  BIN="$TARGET_DIR/x86_64-pc-windows-gnu/release/rustguard.exe"
fi

mkdir -p "$DIST_DIR"
cp "$BIN" "$DIST_DIR/rustguard-windows-amd64.exe"
