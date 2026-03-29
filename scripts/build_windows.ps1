$ErrorActionPreference = "Stop"
# Repository root used as canonical base for relative paths.
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
# Cargo manifest patched during versioned release builds.
$manifest = Join-Path $root "app/Cargo.toml"
# Output directory for Windows artifacts consumed by release workflow.
$dist = Join-Path $root "dist/windows"
$targetDir = if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { $env:CARGO_TARGET_DIR } else { Join-Path $root $env:CARGO_TARGET_DIR }
} else {
    Join-Path $root "target"
}
$env:CARGO_TARGET_DIR = $targetDir
# Final executable path produced by release build.
$bin = Join-Path $targetDir "release/rustguard.exe"

if ($args.Count -ne 1) {
    throw "Usage: ./scripts/build_windows.ps1 <version>"
}

$version = $args[0].TrimStart("v")
if ([string]::IsNullOrWhiteSpace($version)) {
    throw "Version cannot be empty"
}

$manifestContent = Get-Content -Path $manifest -Raw
$updatedManifest = [regex]::Replace($manifestContent, '^version\s*=\s*".*"$', "version = `"$version`"", [System.Text.RegularExpressions.RegexOptions]::Multiline, [TimeSpan]::FromSeconds(2))
Set-Content -Path $manifest -Value $updatedManifest

try {
    Push-Location $root
    try {
        cargo build --release --manifest-path app/Cargo.toml
    }
    finally {
        Pop-Location
    }

    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Copy-Item $bin (Join-Path $dist "rustguard-windows-amd64.exe") -Force
}
finally {
    $resetManifest = [regex]::Replace((Get-Content -Path $manifest -Raw), '^version\s*=\s*".*"$', "version = `"0.0.0`"", [System.Text.RegularExpressions.RegexOptions]::Multiline, [TimeSpan]::FromSeconds(2))
    Set-Content -Path $manifest -Value $resetManifest
}
