# Bridge Database Implementation Guide

## ✅ What's Done

Successfully integrated `bridge-wasm` database adapter into `core-library`:

- **Build Size**: 280.29 KB (optimized WASM)
- **Database Adapter**: `WasmDbAdapter` from `bridge-wasm`
- **API**: `JsDatabase.create()` for async initialization
- **Methods**: Full CRUD operations (insert, get, list, delete, query, execute)

## ⚠️ What's Needed: JavaScript Bridge

The `WasmDbAdapter` delegates all SQL operations to JavaScript. You need to implement the `bridgeWasmDb` namespace.

### Required JavaScript Implementation

```javascript
// Global namespace that Rust calls via wasm-bindgen
window.bridgeWasmDb = {
    /**
     * Initialize database connection
     * @param {string} databaseUrl - Connection string (e.g., "sqlite::memory:")
     * @returns {Promise<void>}
     */
    async init(databaseUrl) {
        // Initialize sql.js + IndexedDB
        // Store connection reference
    },

    /**
     * Execute SELECT query
     * @param {string} sql - SQL query
     * @param {Array} params - Query parameters
     * @returns {Promise<Array>} Array of row objects
     */
    async query(sql, params) {
        // Execute query
        // Return rows as: [{ column: value, ... }, ...]
    },

    /**
     * Execute INSERT/UPDATE/DELETE
     * @param {string} sql - SQL statement
     * @param {Array} params - Statement parameters
     * @returns {Promise<number>} Rows affected
     */
    async execute(sql, params) {
        // Execute statement
        // Return number of rows affected
    },

    /**
     * Initialize database (run migrations)
     * @returns {Promise<void>}
     */
    async initialize() {
        // Run schema migrations
        // Create tables: artists, albums, tracks, etc.
    },

    /**
     * Health check
     * @returns {Promise<void>}
     */
    async healthCheck() {
        // Verify connection is alive
    },

    /**
     * Close database connection
     * @returns {Promise<void>}
     */
    async close() {
        // Close and cleanup
    },

    /**
     * Get last inserted row ID
     * @returns {Promise<number>}
     */
    async lastInsertRowid() {
        // Return last insert ID
    },

    // Transaction support
    async beginTransaction() { /* ... */ },
    async commitTransaction(txId) { /* ... */ },
    async rollbackTransaction(txId) { /* ... */ },

    // Migration support
    async getSchemaVersion() { /* ... */ },
    async applyMigration(version, sql) { /* ... */ },
    async isMigrationApplied(version) { /* ... */ }
};
```

## Implementation Options

### Option 1: sql.js + IndexedDB (Recommended)

Use `sql.js` (SQLite compiled to WASM) with IndexedDB for persistence:

```javascript
import initSqlJs from 'sql.js';

let SQL;
let db;

window.bridgeWasmDb = {
    async init(databaseUrl) {
        SQL = await initSqlJs({
            locateFile: file => `https://sql.js.org/dist/${file}`
        });
        
        // Try to load from IndexedDB
        const saved = await loadFromIndexedDB('music-db');
        if (saved) {
            db = new SQL.Database(saved);
        } else {
            db = new SQL.Database();
        }
    },

    async query(sql, params) {
        const results = db.exec(sql, params);
        if (results.length === 0) return [];
        
        const columns = results[0].columns;
        const values = results[0].values;
        
        return values.map(row => {
            const obj = {};
            columns.forEach((col, i) => {
                obj[col] = row[i];
            });
            return obj;
        });
    },

    async execute(sql, params) {
        db.run(sql, params);
        return db.getRowsModified();
    },

    async close() {
        // Save to IndexedDB
        const data = db.export();
        await saveToIndexedDB('music-db', data);
        db.close();
    }
};
```

### Option 2: Native IndexedDB

Implement SQL parser and use IndexedDB directly (more complex).

### Option 3: Absurd-SQL

Use `absurd-sql` for better IndexedDB performance with SQL.

## Schema Required

The database needs this schema (from `core-library/migrations/`):

```sql
-- Artists table
CREATE TABLE artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    sort_name TEXT,
    bio TEXT,
    country TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_artists_normalized_name ON artists(normalized_name);

-- Albums, tracks, playlists tables...
-- See migrations/*.sql
```

## Testing

1. Implement `bridgeWasmDb` namespace
2. Open `db-bridge-demo.html` in browser
3. Click "Initialize Database"
4. Should see: ✅ Database initialized successfully
5. Test CRUD operations

## File Locations

- **WASM build**: `core-library/pkg/core_library_bg.wasm` (280 KB)
- **TypeScript defs**: `core-library/pkg/core_library.d.ts`
- **Demo**: `core-library/db-bridge-demo.html`
- **Adapter source**: `bridge-wasm/src/database.rs`
- **Migrations**: `core-library/migrations/*.sql`

## Architecture

```
┌─────────────────────────────────────────────────┐
│              JavaScript Application              │
│                                                  │
│  import { JsDatabase, JsArtist } from 'pkg'     │
│                                                  │
│  const db = await JsDatabase.create(url);       │
│  await db.insertArtist(artist);                 │
└──────────────────┬──────────────────────────────┘
                   │ wasm-bindgen FFI
┌──────────────────▼──────────────────────────────┐
│           Rust WASM (core-library)               │
│                                                  │
│  JsDatabase → WasmDbAdapter → bridgeWasmDb      │
│                                                  │
└──────────────────┬──────────────────────────────┘
                   │ JavaScript calls
┌──────────────────▼──────────────────────────────┐
│         window.bridgeWasmDb namespace            │
│                                                  │
│  query(sql, params) → sql.js → IndexedDB        │
│  execute(sql, params) → sql.js → IndexedDB      │
└──────────────────────────────────────────────────┘
```

## Next Steps

1. ✅ DONE: Update core-library to use bridge-wasm adapter
2. ✅ DONE: Rebuild WASM (280 KB)
3. ⚠️ TODO: Implement `window.bridgeWasmDb` JavaScript functions
4. ⚠️ TODO: Integrate sql.js + IndexedDB for persistence
5. ⚠️ TODO: Test full CRUD operations in browser

The Rust code is complete and ready. Only the JavaScript bridge needs implementation.
