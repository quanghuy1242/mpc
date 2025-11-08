# WASM Build System

This workspace provides a shared build system for compiling crates to WebAssembly (WASM).

## Prerequisites

1. **wasm-pack** (required)
   ```bash
   cargo install wasm-pack
   ```

2. **binaryen** (optional, for additional optimization)
   - Windows: `scoop install binaryen`
   - macOS: `brew install binaryen`
   - Linux: `sudo apt install binaryen`

## Building for WASM

### Using the Shared Script (Recommended)

From the workspace root:

```powershell
# Build core-library
.\build-wasm.ps1 -Crate core-library

# Build core-playback
.\build-wasm.ps1 -Crate core-playback

# Skip wasm-opt optimization (faster builds)
.\build-wasm.ps1 -Crate core-playback -SkipOptimization
```

### Using Crate-Specific Scripts

Each WASM-enabled crate has a local `build-wasm.ps1` that wraps the shared script:

```powershell
# From core-library/
.\build-wasm.ps1

# From core-playback/
.\build-wasm.ps1
```

## Build Output

Generated files are placed in `<crate>/pkg/`:

- `<crate_name>.js` - JavaScript bindings
- `<crate_name>_bg.wasm` - WebAssembly binary
- `<crate_name>.d.ts` - TypeScript definitions
- `package.json` - NPM package metadata

### Size Comparison

| Crate | WASM Size (unoptimized) | WASM Size (with wasm-opt) |
|-------|-------------------------|---------------------------|
| core-library | ~250 KB | ~200 KB |
| core-playback | ~310 KB | ~250 KB (estimated) |

## Cargo Configuration

### Required Settings

Each WASM-enabled crate needs:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[package.metadata.wasm-pack.profile.release]
wasm-opt = ['-Os', '--enable-bulk-memory', '--enable-nontrapping-float-to-int']
```

### WASM Dependencies

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
bridge-wasm = { path = "../bridge-wasm" }
wasm-bindgen = { workspace = true }
wasm-bindgen-futures = { workspace = true }
js-sys = { workspace = true }
web-sys = { workspace = true }
console_error_panic_hook = { version = "0.1", optional = true }
wee_alloc = { version = "0.4", optional = true }
```

### Release Profile Optimization

**Note:** These settings should be in the workspace root `Cargo.toml`, not individual crates:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Enable Link Time Optimization
codegen-units = 1   # Better optimization
strip = true        # Strip symbols
panic = "abort"     # Smaller panic handler
```

## Adding WASM Support to New Crates

1. Update `Cargo.toml`:
   - Add `[lib]` section with `crate-type = ["cdylib", "rlib"]`
   - Add `[package.metadata.wasm-pack.profile.release]` section
   - Add WASM-specific dependencies under `[target.'cfg(target_arch = "wasm32")'.dependencies]`

2. Create `build-wasm.ps1` wrapper:
   ```powershell
   #!/usr/bin/env pwsh
   $workspaceRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
   & "$workspaceRoot/build-wasm.ps1" -Crate "your-crate-name"
   ```

3. Update the shared script validator:
   - Edit `build-wasm.ps1` at workspace root
   - Add your crate name to the `ValidateSet` parameter

4. Test the build:
   ```powershell
   .\build-wasm.ps1 -Crate your-crate-name
   ```

## Architecture Notes

- **Bridge Pattern**: WASM builds use `bridge-wasm` implementations instead of `bridge-desktop`
- **Async Runtime**: Uses `core-async` which provides WASM-compatible async primitives
- **No Threading**: WASM is single-threaded; all `Mutex` and `Semaphore` types use `Rc<RefCell<T>>` instead of `Arc<Mutex<T>>`
- **No Native Dependencies**: Avoid crates that require system libraries (OpenSSL, SQLite native, etc.)

## Troubleshooting

### Build Errors

**Error: "wasm-pack not found"**
```bash
cargo install wasm-pack
```

**Error: "profiles will be ignored"**
- This is a warning only. Move `[profile.release]` settings to workspace root `Cargo.toml` to silence it.

**Error: "can't find crate for `std`"**
- Ensure `wasm32-unknown-unknown` target is installed:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

### Size Optimization

If WASM bundle is too large:

1. Check feature flags - disable unused features
2. Use `wasm-opt -Oz` for maximum compression
3. Profile with `twiggy` to find bloat:
   ```bash
   cargo install twiggy
   twiggy top pkg/your_crate_bg.wasm
   ```

## References

- [wasm-pack Documentation](https://rustwasm.github.io/wasm-pack/)
- [Rust and WebAssembly Book](https://rustwasm.github.io/book/)
- [wasm-bindgen Guide](https://rustwasm.github.io/wasm-bindgen/)
