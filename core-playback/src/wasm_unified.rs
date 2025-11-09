//! Unified WASM exports for core-playback
//!
//! This module provides a complete WASM interface by re-exporting functionality
//! from core-library and core-runtime with proper namespacing to avoid symbol conflicts.

use wasm_bindgen::prelude::*;

// Re-export types from core-library with aliases
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["core_library"])]
    pub type JsLibrary;
}

// Re-export runtime utilities
pub use core_runtime::wasm::{
    init_panic_hook as playback_init_panic_hook,
};

/// Module metadata - namespaced to avoid conflicts
#[wasm_bindgen(js_name = "playback_module_name")]
pub fn playback_module_name() -> String {
    "core-playback".to_string()
}

#[wasm_bindgen(js_name = "playback_module_version")]
pub fn playback_module_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get all module versions in this bundle
#[wasm_bindgen(js_name = "getBundleInfo")]
pub fn get_bundle_info() -> JsValue {
    let info = serde_json::json!({
        "core_playback": env!("CARGO_PKG_VERSION"),
        "bundle": "core-playback-unified",
        "description": "Audio playback with MP3 and AAC support"
    });
    
    serde_wasm_bindgen::to_value(&info).unwrap_or(JsValue::NULL)
}

// Re-export all playback functionality from wasm.rs
pub use crate::wasm::*;
