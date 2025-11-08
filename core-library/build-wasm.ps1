#!/usr/bin/env pwsh
# Wrapper script for backward compatibility - calls the shared build script

$workspaceRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
& "$workspaceRoot/build-wasm.ps1" -Crate "core-library"
