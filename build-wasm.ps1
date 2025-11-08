#!/usr/bin/env pwsh
# Shared PowerShell script to build any crate for WASM with optimizations

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet("core-library", "core-playback")]
    [string]$Crate,
    
    [Parameter(Mandatory=$false)]
    [switch]$SkipOptimization
)

Write-Host "Building $Crate for WebAssembly..." -ForegroundColor Cyan

# Get workspace root (script location)
$workspaceRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$cratePath = Join-Path $workspaceRoot $Crate

# Verify crate exists
if (-not (Test-Path $cratePath)) {
    Write-Host "Error: Crate '$Crate' not found at $cratePath" -ForegroundColor Red
    exit 1
}

# Change to crate directory
Set-Location $cratePath

# Check if wasm-pack is installed
if (-not (Get-Command wasm-pack -ErrorAction SilentlyContinue)) {
    Write-Host "Error: wasm-pack is not installed!" -ForegroundColor Red
    Write-Host "Install it with: cargo install wasm-pack" -ForegroundColor Yellow
    exit 1
}

Write-Host "Running wasm-pack build..." -ForegroundColor Green

# Build with release profile and web target
wasm-pack build --target web --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "`nBuild failed!" -ForegroundColor Red
    exit 1
}

Write-Host "`nBuild successful!" -ForegroundColor Green

# Optimization step (unless skipped)
if (-not $SkipOptimization) {
    if (Get-Command wasm-opt -ErrorAction SilentlyContinue) {
        Write-Host "`nRunning wasm-opt for additional size optimization..." -ForegroundColor Green
        $wasmFileName = ($Crate -replace '-', '_') + "_bg.wasm"
        $wasmFile = "pkg/$wasmFileName"
        
        if (Test-Path $wasmFile) {
            $originalSize = (Get-Item $wasmFile).Length
            wasm-opt -Oz $wasmFile -o "${wasmFile}.opt"
            
            if ($LASTEXITCODE -eq 0) {
                Move-Item -Force "${wasmFile}.opt" $wasmFile
                $optimizedSize = (Get-Item $wasmFile).Length
                $savedBytes = $originalSize - $optimizedSize
                $savedPercent = [math]::Round(($savedBytes / $originalSize) * 100, 2)
                
                Write-Host "`nOptimization complete!" -ForegroundColor Green
                Write-Host "Original size: $([math]::Round($originalSize / 1KB, 2)) KB" -ForegroundColor Cyan
                Write-Host "Optimized size: $([math]::Round($optimizedSize / 1KB, 2)) KB" -ForegroundColor Cyan
                Write-Host "Saved: $([math]::Round($savedBytes / 1KB, 2)) KB ($savedPercent%)" -ForegroundColor Cyan
            } else {
                Write-Host "Warning: wasm-opt failed, using unoptimized build" -ForegroundColor Yellow
            }
        }
    } else {
        Write-Host "`nNote: wasm-opt not found. Install binaryen for additional size optimization." -ForegroundColor Yellow
        Write-Host "  Windows: scoop install binaryen" -ForegroundColor Yellow
        Write-Host "  macOS: brew install binaryen" -ForegroundColor Yellow
        Write-Host "  Linux: sudo apt install binaryen" -ForegroundColor Yellow
    }
} else {
    Write-Host "`nSkipping wasm-opt optimization (--SkipOptimization flag set)" -ForegroundColor Yellow
}

# Display package info
Write-Host "`nGenerated files in $Crate/pkg/:" -ForegroundColor Cyan
Get-ChildItem pkg/ | ForEach-Object {
    $size = if ($_.PSIsContainer) { "DIR" } else { "$([math]::Round($_.Length / 1KB, 2)) KB" }
    Write-Host "  $($_.Name) - $size"
}

Write-Host "`nTypeScript definitions: pkg/$($Crate -replace '-', '_').d.ts" -ForegroundColor Green
Write-Host "Ready to use in your web application!" -ForegroundColor Green

# Return to workspace root
Set-Location $workspaceRoot
