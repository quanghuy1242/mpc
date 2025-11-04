# Suggested Commands for Music Platform Core

## Building

### Build All Crates
```bash
cargo build --workspace
```

### Build with All Features
```bash
cargo build --workspace --all-features
```

### Build for Release
```bash
cargo build --workspace --release
```

### Build Specific Crate
```bash
cargo build -p core-runtime
cargo build -p bridge-desktop
```

### Build Desktop with Default Features
```bash
cargo build --release --features desktop-shims
```

## Testing

### Run All Tests
```bash
cargo test --workspace
```

### Run Tests with All Features
```bash
cargo test --workspace --all-features
```

### Run Tests for Specific Crate
```bash
cargo test -p core-runtime
cargo test -p bridge-traits
```

### Run Specific Test
```bash
cargo test test_name
```

### Run Tests with Output
```bash
cargo test -- --nocapture
```

### Run Integration Tests Only
```bash
cargo test --test '*'
```

## Code Quality

### Format Code
```bash
cargo fmt --all
```

### Check Formatting Without Modifying
```bash
cargo fmt --all -- --check
```

### Run Clippy (Linter)
```bash
cargo clippy --all-targets --all-features
```

### Run Clippy with Warnings as Errors
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Check Without Building
```bash
cargo check --workspace
```

## Documentation

### Generate Documentation
```bash
cargo doc --no-deps --all-features
```

### Generate and Open Documentation
```bash
cargo doc --no-deps --all-features --open
```

### Check Documentation Links
```bash
cargo doc --no-deps --all-features --document-private-items
```

## Running Examples

### Run Logging Demo
```bash
cargo run --example logging_demo
```

### Run Example from Specific Crate
```bash
cargo run -p core-runtime --example logging_demo
```

## Platform-Specific Builds

### Mobile FFI Generation (iOS)
```bash
cargo build --release --target aarch64-apple-ios --features ffi
```

### Mobile FFI Generation (Android)
```bash
cargo build --release --target aarch64-linux-android --features ffi
```

### WASM Build
```bash
wasm-pack build --target web --features wasm core-wasm/
```

## Database Operations

### Run Migrations (when sqlx is fully set up)
```bash
sqlx migrate run --database-url sqlite://local.db
```

### Create New Migration
```bash
sqlx migrate add <migration_name>
```

### Check Migrations Status
```bash
sqlx migrate info --database-url sqlite://local.db
```

## Dependency Management

### Update Dependencies
```bash
cargo update
```

### Add Dependency to Workspace
Edit `Cargo.toml` workspace.dependencies section

### Add Dependency to Specific Crate
```bash
cd <crate-name>
cargo add <dependency-name>
```

### Remove Unused Dependencies
```bash
cargo install cargo-udeps
cargo +nightly udeps --all-targets
```

### Check for Security Vulnerabilities
```bash
cargo audit
```

## Clean & Maintenance

### Clean Build Artifacts
```bash
cargo clean
```

### Check Unused Dependencies
```bash
cargo tree --duplicates
```

### View Build Graph
```bash
cargo tree
```

### Show Outdated Dependencies
```bash
cargo install cargo-outdated
cargo outdated
```

## Performance & Optimization

### Build with Profile
```bash
cargo build --profile release-with-debug
```

### Run Benchmarks (when implemented)
```bash
cargo bench
```

### Check Binary Size
```bash
cargo install cargo-bloat
cargo bloat --release
```

## Debugging

### Run with Backtrace
```bash
RUST_BACKTRACE=1 cargo run
RUST_BACKTRACE=full cargo test
```

### Run with Logging
```bash
RUST_LOG=debug cargo run
RUST_LOG=core_runtime=trace,core_auth=debug cargo run
```

### Expand Macros
```bash
cargo expand
```

## FFI Binding Generation (when implemented)

### Generate Swift Bindings (iOS/macOS)
```bash
cargo run --bin uniffi-bindgen generate \
    --library target/release/libcore.dylib \
    --language swift \
    --out-dir bindings/swift
```

### Generate Kotlin Bindings (Android)
```bash
cargo run --bin uniffi-bindgen generate \
    --library target/release/libcore.so \
    --language kotlin \
    --out-dir bindings/kotlin
```

## Git Workflow Commands (Linux)

### Check Status
```bash
git status
```

### View Changes
```bash
git diff
```

### Stage Changes
```bash
git add .
git add <file>
```

### Commit
```bash
git commit -m "message"
```

### Push
```bash
git push origin main
```

### Pull Latest
```bash
git pull
```

### Create Branch
```bash
git checkout -b feature/branch-name
```

## Linux System Commands

### List Files
```bash
ls -la
```

### Find Files
```bash
find . -name "*.rs"
```

### Search in Files
```bash
grep -r "pattern" .
rg "pattern"  # if ripgrep installed
```

### Navigate
```bash
cd /path/to/project
cd ..
pwd
```

### View File Content
```bash
cat file.rs
less file.rs
head -n 20 file.rs
tail -n 20 file.rs
```
