#!/usr/bin/env pwsh
# PowerShell script to build core-library for WASM with optimizations

Write-Host "Building core-library for WebAssembly..." -ForegroundColor Cyan

# Change to core-library directory
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptPath

# Check if wasm-pack is installed
if (-not (Get-Command wasm-pack -ErrorAction SilentlyContinue)) {
    Write-Host "Error: wasm-pack is not installed!" -ForegroundColor Red
    Write-Host "Install it with: cargo install wasm-pack" -ForegroundColor Yellow
    exit 1
}

Write-Host "Running wasm-pack build..." -ForegroundColor Green

# Build with release profile and web target
wasm-pack build --target web --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "`nBuild successful!" -ForegroundColor Green
    
    # Check if wasm-opt is available for additional optimization
    if (Get-Command wasm-opt -ErrorAction SilentlyContinue) {
        Write-Host "`nRunning wasm-opt for additional size optimization..." -ForegroundColor Green
        $wasmFile = "pkg/core_library_bg.wasm"
        
        if (Test-Path $wasmFile) {
            $originalSize = (Get-Item $wasmFile).Length
            wasm-opt -Oz $wasmFile -o "${wasmFile}.opt"
            
            if ($LASTEXITCODE -eq 0) {
                Move-Item -Force "${wasmFile}.opt" $wasmFile
                $optimizedSize = (Get-Item $wasmFile).Length
                $savedBytes = $originalSize - $optimizedSize
                $savedPercent = [math]::Round(($savedBytes / $originalSize) * 100, 2)
                
                Write-Host "`nOptimization complete!" -ForegroundColor Green
                Write-Host "Original size: $($originalSize / 1KB) KB" -ForegroundColor Cyan
                Write-Host "Optimized size: $($optimizedSize / 1KB) KB" -ForegroundColor Cyan
                Write-Host "Saved: $($savedBytes / 1KB) KB ($savedPercent%)" -ForegroundColor Cyan
            }
        }
    } else {
        Write-Host "`nNote: wasm-opt not found. Install binaryen for additional size optimization." -ForegroundColor Yellow
    }
    
    # Display package info
    Write-Host "`nGenerated files in pkg/:" -ForegroundColor Cyan
    Get-ChildItem pkg/ | ForEach-Object {
        $size = if ($_.PSIsContainer) { "DIR" } else { "$([math]::Round($_.Length / 1KB, 2)) KB" }
        Write-Host "  $($_.Name) - $size"
    }
    
    Write-Host "`nTypeScript definitions: pkg/core_library.d.ts" -ForegroundColor Green
    Write-Host "Ready to use in your web application!" -ForegroundColor Green
} else {
    Write-Host "`nBuild failed!" -ForegroundColor Red
    exit 1
}
