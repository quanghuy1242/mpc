#!/usr/bin/env pwsh
# Wrapper script - calls the shared build script from workspace root

$workspaceRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
& "$workspaceRoot/build-wasm.ps1" -Crate "core-playback"
