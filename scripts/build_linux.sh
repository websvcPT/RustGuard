#!/usr/bin/env bash
set -euo pipefail

# Repository root used as canonical base for relative paths.
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
# Cargo manifest patched during versioned release builds.
APP_MANIFEST="$ROOT_DIR/app/Cargo.toml"
# Output directory for Linux artifacts consumed by release workflow.
DIST_DIR="$ROOT_DIR/dist/linux"
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

# Final executable path produced by release build.
BIN="$TARGET_DIR/release/rustguard"
# Temporary package root used to construct a .deb payload tree.
PKG_ROOT="$ROOT_DIR/dist/pkg"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <version>" >&2
  exit 1
fi

VERSION="${1#v}"
if [[ -z "$VERSION" ]]; then
  echo "Version cannot be empty" >&2
  exit 1
fi

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "dpkg-deb is required (install the dpkg package)." >&2
  exit 1
fi

# Updates the first `version = "..."` entry in the app Cargo manifest.
set_manifest_version() {
  local new_version="$1"
  sed -i -E "0,/^version = \".*\"/s//version = \"$new_version\"/" "$APP_MANIFEST"
}

rm -rf "$PKG_ROOT"
if [[ "${RUSTGUARD_MANIFEST_ALREADY_PATCHED:-0}" == "1" ]]; then
  trap 'rm -rf "$PKG_ROOT"' EXIT
else
  trap 'set_manifest_version "0.0.0"; rm -rf "$PKG_ROOT"' EXIT
  set_manifest_version "$VERSION"
fi

cargo build --release --manifest-path "$APP_MANIFEST"

mkdir -p \
  "$DIST_DIR" \
  "$PKG_ROOT/usr/bin" \
  "$PKG_ROOT/usr/lib/rustguard" \
  "$PKG_ROOT/usr/share/applications" \
  "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps" \
  "$PKG_ROOT/usr/share/polkit-1/actions" \
  "$PKG_ROOT/DEBIAN"
cp "$BIN" "$DIST_DIR/rustguard-linux-amd64"
chmod +x "$DIST_DIR/rustguard-linux-amd64"

cp "$BIN" "$PKG_ROOT/usr/lib/rustguard/rustguard-bin"
cat > "$PKG_ROOT/usr/bin/rustguard" <<'LAUNCHER'
#!/bin/sh
exec pkexec /usr/lib/rustguard/rustguard-bin "$@"
LAUNCHER
chmod +x "$PKG_ROOT/usr/bin/rustguard"

cp "$ROOT_DIR/app/Icon/rustguard_logo_original.png" "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps/rustguard.png"

cat > "$PKG_ROOT/usr/share/applications/rustguard.desktop" <<DESKTOP
[Desktop Entry]
Name=RustGuard
Comment=WireGuard tunnel manager
Exec=/usr/bin/rustguard
Icon=rustguard
Terminal=false
Type=Application
Categories=Network;Utility;
StartupNotify=true
DESKTOP

cat > "$PKG_ROOT/usr/share/polkit-1/actions/net.websvc.rustguard.policy" <<'POLICY'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="org.freedesktop.policykit.pkexec.rustguard">
    <description>RustGuard WireGuard Manager</description>
    <message>Authentication is required to run RustGuard</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/lib/rustguard/rustguard-bin</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
POLICY

cat > "$PKG_ROOT/DEBIAN/control" <<CONTROL
Package: rustguard
Version: ${VERSION}
Section: net
Priority: optional
Architecture: amd64
Maintainer: WebSVC
Description: RustGuard WireGuard client GUI
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1
CONTROL

dpkg-deb --build "$PKG_ROOT" "$DIST_DIR/rustguard_${VERSION}_amd64.deb"
