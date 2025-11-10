# Refactoring Plan: Unifying `Arc` and `Rc` with `PlatformArc`

## 1. Objective

The goal of this refactoring is to unify the shared ownership patterns across the entire codebase. Currently, `std::sync::Arc` is used for multi-threaded contexts (native) and `std::rc::Rc` is used for single-threaded contexts (WASM). While functionally correct, this approach leads to code divergence and increases maintenance overhead.

By centralizing a `PlatformArc` abstraction, we can write code once that is both thread-safe on native targets and maximally performant (non-atomic) on WASM targets.

## 2. Phase 1: Create Central Abstractions in `bridge-traits`

The `bridge-traits` crate is a dependency for most other crates in the project, making it the ideal location for our platform-specific abstractions.

### Action: Create `bridge-traits/src/platform.rs`

Create a new file with the following content:

```rust
//! Platform-specific type aliases and traits for handling
//! multi-threaded vs. single-threaded environments.

//
// --- Platform-specific Smart Pointers ---
//

/// A platform-agnostic, reference-counted smart pointer.
///
/// This will be `std::sync::Arc` on multi-threaded targets (like desktop)
/// and `std::rc::Rc` on single-threaded targets (like wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub type PlatformArc<T> = std::sync::Arc<T>;
#[cfg(target_arch = "wasm32")]
pub type PlatformArc<T> = std::rc::Rc<T>;

//
// --- Platform-specific Trait Bounds ---
//

/// A platform-agnostic trait alias for `Send + Sync`.
///
/// This bound is required for types that need to be shared across threads.
/// On `wasm32`, this bound is not needed and is an empty trait. This is
/// crucial for creating trait objects (`dyn Trait`) that can be used with
/// `PlatformArc` on both platforms.
#[cfg(not(target_arch = "wasm32"))]
pub trait PlatformSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> PlatformSendSync for T {}

#[cfg(target_arch = "wasm32")]
pub trait PlatformSendSync: ?Sized {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> PlatformSendSync for T {}
```

### Action: Expose the new module

In `bridge-traits/src/lib.rs`, add the following line to make the module public:

```rust
pub mod platform;
```

## 3. Phase 2: Global Code Refactoring

The next step is to replace all direct usages of `Arc` and `Rc` with `PlatformArc`.

### Action: Replace `Arc`, `Rc`, and local aliases

For each crate in the project, perform the following changes:

1.  **Remove Local Definitions:** Delete any local `type PlatformArc<T> = ...` definitions, such as the one found in `core-playback/src/cache/manager.rs`.

2.  **Update `use` Statements:**
    *   Remove `use std::sync::Arc;` and `use std::rc::Rc;`.
    *   Add `use bridge_traits::platform::PlatformArc;`.

3.  **Replace Smart Pointers:**
    *   Find and replace all instances of `Arc<` with `PlatformArc<`.
    *   Find and replace all instances of `Rc<` with `PlatformArc<`.

4.  **Update Trait Bounds:**
    *   For generic functions or structs that use trait objects like `PlatformArc<dyn MyTrait>`, you may need to update the trait definition or bounds.
    *   Change bounds from `T: Send + Sync` to `T: PlatformSendSync` where it makes sense in shared code.
    *   For trait definitions, change `pub trait MyTrait: Send + Sync` to `pub trait MyTrait: PlatformSendSync`.

### Target Crates and Files:

Based on the previous search, the following crates and files will require changes. This is not an exhaustive list, and a project-wide search-and-replace is recommended.

*   `bridge-desktop`
*   `bridge-wasm`
*   `core-auth`
*   `core-library`
*   `core-metadata`
*   `core-playback`
*   `core-runtime`
*   `core-service`
*   `core-sync`
*   `provider-google-drive`

## 4. Phase 3: Verification

After refactoring, it is critical to ensure that the application still compiles and functions correctly on both native and WASM targets.

### Recommended Commands:

1.  **Check Native Build:**
    ```sh
    cargo check
    ```

2.  **Run Native Tests:**
    ```sh
    cargo test --all-features
    ```

3.  **Check WASM Build:**
    ```sh
    cargo check --target wasm32-unknown-unknown --all-features
    ```

4.  **Run WASM Tests:**
    *Execute the project's standard command for running WASM tests.*
    ```sh
    # Example command, replace with project's actual script
    wasm-pack test --node
    ```

By following this plan, the codebase will be significantly cleaner, more maintainable, and better structured for cross-platform development.
