//! Example demonstrating WasmFileSystem usage
//!
//! This example shows how to use the WasmFileSystem to perform various
//! file operations in a WebAssembly environment.
//!
//! TODO: This example is temporarily commented out until Task 5 in docs/immediate_todo.md
//! is completed. The issue is that `FileSystemAccess` trait requires `Send + Sync` bounds,
//! but WASM/JavaScript objects cannot satisfy these bounds. Once the trait is updated with
//! conditional bounds for WASM targets, uncomment this example.
//!
//! See: docs/immediate_todo.md - Task 5: Fix WASM Trait Compatibility Issues

// Stub main function to satisfy cargo's example requirements
fn main() {
    eprintln!("This example is temporarily disabled. See docs/immediate_todo.md - Task 5");
}

/*
use bridge_traits::storage::FileSystemAccess;
use bridge_wasm::WasmFileSystem;
use bytes::Bytes;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    // Setup panic hook for better error messages
    console_error_panic_hook::set_once();

    // Initialize the file system
    let fs = WasmFileSystem::new("music-app")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&"File system initialized".into());

    // Get standard directories
    let cache_dir = fs
        .get_cache_directory()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let data_dir = fs
        .get_data_directory()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&format!("Cache directory: {:?}", cache_dir).into());
    web_sys::console::log_1(&format!("Data directory: {:?}", data_dir).into());

    // Example 1: Write and read a simple text file
    let playlist_path = data_dir.join("playlists/favorites.json");
    let playlist_data = r#"{
        "name": "My Favorites",
        "tracks": [
            {"title": "Song 1", "artist": "Artist 1"},
            {"title": "Song 2", "artist": "Artist 2"}
        ]
    }"#;

    fs.write_file(&playlist_path, Bytes::from(playlist_data))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&"Playlist written successfully".into());

    // Read it back
    let read_data = fs
        .read_file(&playlist_path)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(
        &format!(
            "Read playlist: {}",
            String::from_utf8_lossy(&read_data)
        )
        .into(),
    );

    // Example 2: Create and list directory contents
    let music_dir = data_dir.join("music");
    fs.create_dir_all(&music_dir)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    // Create some "music files" (just mock data)
    let songs = vec![
        ("song1.mp3", "This is song 1 data"),
        ("song2.mp3", "This is song 2 data"),
        ("song3.mp3", "This is song 3 data"),
    ];

    for (filename, content) in &songs {
        let file_path = music_dir.join(filename);
        fs.write_file(&file_path, Bytes::from(*content))
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
    }

    web_sys::console::log_1(&"Music files created".into());

    // List the directory
    let entries = fs
        .list_directory(&music_dir)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&format!("Found {} files:", entries.len()).into());
    for entry in entries {
        web_sys::console::log_1(&format!("  - {:?}", entry.file_name().unwrap()).into());
    }

    // Example 3: Check file metadata
    let song_path = music_dir.join("song1.mp3");
    let metadata = fs
        .metadata(&song_path)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(
        &format!(
            "Metadata for song1.mp3: size={} bytes, is_directory={}",
            metadata.size, metadata.is_directory
        )
        .into(),
    );

    // Example 4: Append to a log file
    let log_file = cache_dir.join("app.log");
    fs.write_file(&log_file, Bytes::from("App started\n"))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    fs.append_file(&log_file, Bytes::from("Initialized file system\n"))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    fs.append_file(&log_file, Bytes::from("Created music files\n"))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let log_content = fs
        .read_file(&log_file)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(
        &format!("Log file:\n{}", String::from_utf8_lossy(&log_content)).into(),
    );

    // Example 5: Calculate directory size
    let total_size = fs
        .directory_size(&music_dir)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&format!("Total music directory size: {} bytes", total_size).into());

    // Example 6: Delete a file
    fs.delete_file(&song_path)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&"Deleted song1.mp3".into());

    // Example 7: Demonstrate large file handling (chunked storage)
    let large_file = cache_dir.join("large-download.bin");
    let large_data: Vec<u8> = (0..2_000_000).map(|i| (i % 256) as u8).collect();

    web_sys::console::log_1(&"Writing large file (2MB)...".into());
    fs.write_file(&large_file, Bytes::from(large_data.clone()))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    web_sys::console::log_1(&"Reading large file back...".into());
    let read_large = fs
        .read_file(&large_file)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    if read_large.len() == large_data.len() {
        web_sys::console::log_1(&"Large file verified successfully!".into());
    } else {
        web_sys::console::error_1(&"Large file size mismatch!".into());
    }

    web_sys::console::log_1(&"All examples completed successfully!".into());

    Ok(())
}
*/
