# WASM Build Explanation

## How It Works Without Database

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    core-library (Rust)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────┐    ┌────────────────┐    ┌──────────────┐  │
│  │   models.rs   │    │  db.rs         │    │  wasm.rs     │  │
│  │               │    │                │    │              │  │
│  │ pub struct    │    │ #[cfg(not(    │    │ #[wasm_      │  │
│  │  Artist {     │    │  wasm32))]     │    │  bindgen]    │  │
│  │   id: String  │    │                │    │              │  │
│  │   name: String│    │ SQLite pool    │    │ pub struct   │  │
│  │   ...         │    │ Migrations     │    │  JsArtist    │  │
│  │ }             │    │ Queries        │    │              │  │
│  │               │    │                │    │              │  │
│  │ impl Artist { │    └────────────────┘    └──────────────┘  │
│  │   pub fn new()│          ↑                      ↑          │
│  │   pub fn      │          │                      │          │
│  │   validate()  │    EXCLUDED from              INCLUDED     │
│  │ }             │    WASM build                 in WASM      │
│  └───────────────┘                                            │
│         ↑                                                      │
│         │                                                      │
│    Used by both                                                │
│    native & WASM                                              │
└─────────────────────────────────────────────────────────────────┘
```

## First Build: Models Only (223 KB)

### What Was Exported

```rust
// src/wasm.rs

#[wasm_bindgen]
pub struct JsArtist {
    inner: Artist,  // ← wraps the model from models.rs
}

#[wasm_bindgen]
impl JsArtist {
    #[wasm_bindgen(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            inner: Artist::new(name)  // ← calls models::Artist::new()
        }
    }
    
    pub fn name(&self) -> String {
        self.inner.name.clone()  // ← accesses model field
    }
    
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner)  // ← serializes model
    }
}
```

### JavaScript Usage (First Build)

```javascript
// Each instance is independent
const artist1 = new JsArtist("The Beatles");
const artist2 = new JsArtist("Pink Floyd");

// No way to store them together
// No way to query them later
// They live in JS memory only

console.log(artist1.name());  // "The Beatles"
console.log(artist1.toJson()); // '{"id":"...","name":"The Beatles",...}'
```

## Second Build: Added Database (274 KB)

### What Was Added

```rust
// src/adapters/wasm_storage.rs (NEW FILE!)

pub struct WasmStorage {
    data: StorageData,
}

pub struct StorageData {
    pub artists: HashMap<String, Artist>,   // ← stores Artist models
    pub albums: HashMap<String, Album>,
    pub tracks: HashMap<String, Track>,
    pub playlists: HashMap<String, Playlist>,
}

impl WasmStorage {
    pub fn insert_artist(&mut self, artist: Artist) -> Result<(), String> {
        self.data.artists.insert(artist.id.clone(), artist);
        Ok(())
    }
    
    pub fn get_artist(&self, id: &str) -> Option<&Artist> {
        self.data.artists.get(id)
    }
    
    pub fn list_artists(&self) -> Vec<&Artist> {
        self.data.artists.values().collect()
    }
}
```

### Exported to JavaScript

```rust
// src/wasm.rs (ADDED 200+ lines)

#[wasm_bindgen]
pub struct JsDatabase {
    storage: Rc<RefCell<WasmStorage>>,  // ← wraps the storage
}

#[wasm_bindgen]
impl JsDatabase {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            storage: Rc::new(RefCell::new(WasmStorage::new()))
        }
    }
    
    #[wasm_bindgen(js_name = insertArtist)]
    pub fn insert_artist(&self, artist: &JsArtist) -> Result<(), JsValue> {
        self.storage
            .borrow_mut()
            .insert_artist(artist.inner.clone())  // ← stores in HashMap
            .map_err(|e| JsValue::from_str(&e))
    }
    
    #[wasm_bindgen(js_name = listArtists)]
    pub fn list_artists(&self) -> Vec<JsArtist> {
        self.storage
            .borrow()
            .list_artists()  // ← retrieves from HashMap
            .into_iter()
            .map(|a| JsArtist { inner: a.clone() })
            .collect()
    }
}
```

### JavaScript Usage (Second Build)

```javascript
// Create a database
const db = new JsDatabase();

// Store artists in Rust HashMap
const artist1 = new JsArtist("The Beatles");
const artist2 = new JsArtist("Pink Floyd");
db.insertArtist(artist1);
db.insertArtist(artist2);

// Query later
const allArtists = db.listArtists();
console.log(allArtists.length); // 2

// Persist to IndexedDB
const json = db.toJson();
localStorage.setItem('db', json);

// Restore
const db2 = JsDatabase.loadFromJson(json);
```

## Key Differences

### Memory Layout

**First Build:**
```
JavaScript Heap
├── artist1 instance (JsArtist wrapper)
│   └── WASM memory → Artist struct
├── artist2 instance (JsArtist wrapper)
│   └── WASM memory → Artist struct
└── No connection between them
```

**Second Build:**
```
JavaScript Heap
└── db instance (JsDatabase wrapper)
    └── WASM memory
        └── WasmStorage
            └── HashMap {
                "uuid-1" → Artist { name: "The Beatles" }
                "uuid-2" → Artist { name: "Pink Floyd" }
            }
```

## Why No Native DB in WASM?

### src/lib.rs Controls What's Compiled

```rust
// This code is NEVER compiled for WASM:
#[cfg(not(target_arch = "wasm32"))]
pub mod db;  // SQLite connection pooling

#[cfg(not(target_arch = "wasm32"))]
pub use adapters::SqliteAdapter;  // Native SQLite driver

#[cfg(not(target_arch = "wasm32"))]
pub use query::LibraryQueryService;  // Complex SQL queries

// These ARE compiled for all targets (including WASM):
pub use models::{Artist, Album, Track};  // Just structs!
```

### Why This Works

1. **Models are pure Rust structs** - no external dependencies
   ```rust
   pub struct Artist {
       pub id: String,
       pub name: String,
       // ... just data!
   }
   ```

2. **WASM doesn't have access to:**
   - File system (no SQLite database files)
   - Native C libraries (sqlx uses libsqlite3)
   - Thread pools (sqlx uses tokio runtime)

3. **HashMap is pure Rust** - works everywhere
   ```rust
   use std::collections::HashMap;  // ← in Rust std library
   ```

## File Size Breakdown

| Component | First Build | Second Build | What It Does |
|-----------|-------------|--------------|--------------|
| Models (Artist, Album, etc.) | 150 KB | 150 KB | Data structures |
| JSON serialization | 40 KB | 40 KB | serde_json |
| WASM bindings | 33 KB | 33 KB | wasm-bindgen |
| **HashMap storage** | **-** | **40 KB** | **WasmStorage** |
| **CRUD operations** | **-** | **11 KB** | **JsDatabase methods** |
| **Total** | **223 KB** | **274 KB** | |

## Summary

**First Build:**
- Exported **wrappers** around existing models
- No storage mechanism
- Just create/validate/serialize individual instances

**Second Build:**
- Added **HashMap-based storage** (wasm_storage.rs)
- Added **database wrapper** (JsDatabase)
- Added **CRUD operations** (insert/get/list/update/delete)
- Size increased by **51 KB** for full functionality

**Both builds:**
- Use the **same models.rs** structs
- Excluded native database code via `#[cfg(not(target_arch = "wasm32"))]`
- Work without external SQL libraries
