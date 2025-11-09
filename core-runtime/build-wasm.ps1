# Build script for core-runtime WASM package
#
# This script builds the core-runtime Rust crate to WebAssembly
# and generates TypeScript bindings.

param(
    [switch]$Release,
    [switch]$Dev
)

$ErrorActionPreference = "Stop"

Write-Host "`n=== Building core-runtime WASM ===" -ForegroundColor Cyan

# Determine build profile
$Profile = if ($Release) { "release" } else { "dev" }
$ProfileFlag = if ($Release) { "--release" } else { "" }

Write-Host "Profile: $Profile" -ForegroundColor Yellow

# Check if wasm-pack is installed
if (!(Get-Command wasm-pack -ErrorAction SilentlyContinue)) {
    Write-Host "Error: wasm-pack not found!" -ForegroundColor Red
    Write-Host "Install it with: cargo install wasm-pack" -ForegroundColor Yellow
    exit 1
}

# Build with wasm-pack
Write-Host "`nBuilding with wasm-pack..." -ForegroundColor Green

$BuildArgs = @(
    "build",
    "--target", "web",
    "--out-dir", "pkg",
    "--",
    "--features", "wasm"
)

if ($Release) {
    $BuildArgs += "--release"
} else {
    $BuildArgs += "--dev"
}

# Skip wasm-opt to avoid bulk-memory issues
$env:WASM_PACK_SKIP_OPT = "1"

& wasm-pack @BuildArgs

if ($LASTEXITCODE -ne 0) {
    Write-Host "`nBuild failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "`n✓ Build successful!" -ForegroundColor Green
Write-Host "`nOutput directory: pkg/" -ForegroundColor Cyan
Write-Host "Files generated:" -ForegroundColor Cyan
Get-ChildItem pkg\ | ForEach-Object {
    Write-Host "  - $($_.Name)" -ForegroundColor White
}

Write-Host "`n=== Build Complete ===" -ForegroundColor Green
