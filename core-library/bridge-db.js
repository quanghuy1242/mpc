/**
 * JavaScript bridge for WasmDbAdapter
 * 
 * This implements the bridgeWasmDb namespace that the Rust WasmDbAdapter calls.
 * Uses NATIVE IndexedDB API only - NO external dependencies!
 */

let db;
let transactions = new Map();
let nextTxId = 1;

// In-memory storage for tables
const tables = {
    artists: [],
    albums: [],
    tracks: [],
    playlists: [],
    playlist_tracks: [],
    _migrations: [] // Track applied migrations
};

// Schema metadata (column definitions from CREATE TABLE)
const schema = {};

/**
 * Open IndexedDB for persistence
 */
async function openIndexedDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open('MusicLibraryDB', 1);
        
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
        
        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            
            // Create object stores for each table
            if (!db.objectStoreNames.contains('artists')) {
                db.createObjectStore('artists', { keyPath: 'id' });
            }
            if (!db.objectStoreNames.contains('albums')) {
                db.createObjectStore('albums', { keyPath: 'id' });
            }
            if (!db.objectStoreNames.contains('tracks')) {
                db.createObjectStore('tracks', { keyPath: 'id' });
            }
            if (!db.objectStoreNames.contains('playlists')) {
                db.createObjectStore('playlists', { keyPath: 'id' });
            }
            if (!db.objectStoreNames.contains('playlist_tracks')) {
                db.createObjectStore('playlist_tracks', { keyPath: ['playlist_id', 'track_id'] });
            }
            if (!db.objectStoreNames.contains('_migrations')) {
                db.createObjectStore('_migrations', { keyPath: 'version' });
            }
        };
    });
}

/**
 * Load all data from IndexedDB
 */
async function loadFromIndexedDB() {
    try {
        const idb = await openIndexedDB();
        
        for (const tableName of Object.keys(tables)) {
            if (idb.objectStoreNames.contains(tableName)) {
                const tx = idb.transaction([tableName], 'readonly');
                const store = tx.objectStore(tableName);
                const request = store.getAll();
                
                tables[tableName] = await new Promise((resolve, reject) => {
                    request.onsuccess = () => resolve(request.result || []);
                    request.onerror = () => reject(request.error);
                });
            }
        }
        
        console.log('[bridgeWasmDb] Loaded data from IndexedDB:', {
            artists: tables.artists.length,
            albums: tables.albums.length,
            tracks: tables.tracks.length,
            playlists: tables.playlists.length
        });
    } catch (error) {
        console.warn('Failed to load from IndexedDB:', error);
    }
}

/**
 * Save all data to IndexedDB
 */
async function saveToIndexedDB() {
    try {
        const idb = await openIndexedDB();
        
        for (const [tableName, data] of Object.entries(tables)) {
            // Skip internal tables
            if (tableName === '_migrations') continue;
            
            if (idb.objectStoreNames.contains(tableName)) {
                const tx = idb.transaction([tableName], 'readwrite');
                const store = tx.objectStore(tableName);
                
                // Clear and repopulate
                await new Promise((resolve, reject) => {
                    const clearRequest = store.clear();
                    clearRequest.onsuccess = () => resolve();
                    clearRequest.onerror = () => reject(clearRequest.error);
                });
                
                for (const item of data) {
                    await new Promise((resolve, reject) => {
                        const putRequest = store.put(item);
                        putRequest.onsuccess = () => {
                            console.log(`[bridgeWasmDb] PUT success for ${tableName}:`, item.id || 'no-id');
                            resolve();
                        };
                        putRequest.onerror = () => {
                            console.error(`[bridgeWasmDb] PUT failed for ${tableName}:`, putRequest.error, item);
                            reject(putRequest.error);
                        };
                    });
                }
                
                console.log(`[bridgeWasmDb] Saved ${data.length} records to IndexedDB table: ${tableName}`);
            }
        }
        
        console.log('[bridgeWasmDb] ✅ All data saved to IndexedDB');
    } catch (error) {
        console.error('Failed to save to IndexedDB:', error);
    }
}

/**
 * Convert QueryValue from Rust to JavaScript value
 */
function fromQueryValue(value) {
    if (value === null || value === undefined) return null;
    if (typeof value === 'object') {
        if ('Text' in value) return value.Text;
        if ('Integer' in value) return value.Integer;
        if ('Real' in value) return value.Real;
        if ('Blob' in value) return new Uint8Array(value.Blob);
        if ('Null' in value) return null;
    }
    return value;
}

/**
 * Convert JavaScript value to QueryValue for Rust
 */
function toQueryValue(value) {
    if (value === null || value === undefined) return { Null: null };
    if (typeof value === 'string') return { Text: value };
    if (typeof value === 'number') {
        return Number.isInteger(value) ? { Integer: value } : { Real: value };
    }
    if (value instanceof Uint8Array) return { Blob: Array.from(value) };
    return { Text: String(value) };
}

/**
 * Parse table column definitions from CREATE TABLE statement
 */
function parseTableColumns(columnsDef) {
    const columns = [];
    const lines = columnsDef.split(',');
    
    for (let line of lines) {
        line = line.trim();
        
        // Skip constraints, indexes, foreign keys
        if (line.toUpperCase().startsWith('CONSTRAINT') ||
            line.toUpperCase().startsWith('FOREIGN KEY') ||
            line.toUpperCase().startsWith('PRIMARY KEY') ||
            line.toUpperCase().startsWith('CHECK')) {
            continue;
        }
        
        // Extract column name and type
        const parts = line.split(/\s+/);
        if (parts.length >= 2) {
            const name = parts[0];
            const type = parts[1].toUpperCase();
            const isPrimaryKey = line.toUpperCase().includes('PRIMARY KEY');
            const notNull = line.toUpperCase().includes('NOT NULL');
            
            columns.push({
                name,
                type,
                isPrimaryKey,
                notNull
            });
        }
    }
    
    return columns;
}

/**
 * Simple SQL parser (handles basic SELECT, INSERT, UPDATE, DELETE)
 */
function parseSQL(sql, params) {
    const sqlUpper = sql.trim().toUpperCase();
    const jsParams = params.map(fromQueryValue);
    
    // COUNT queries MUST be checked BEFORE SELECT - handle "SELECT COUNT(*) as count FROM table"
    if (sqlUpper.includes('COUNT(')) {
        const fromMatch = sql.match(/FROM\s+(\w+)/i);
        if (!fromMatch) return [];
        
        const tableName = fromMatch[1];
        const table = tables[tableName] || [];
        
        // Check if there's an alias like "as count" or just "count"
        const aliasMatch = sql.match(/COUNT\([^)]*\)\s+(?:as\s+)?(\w+)/i);
        const columnName = aliasMatch ? aliasMatch[1] : 'count';
        
        // Apply WHERE clause if present
        let count = table.length;
        const whereMatch = sql.match(/WHERE\s+(.+?)(?:ORDER|LIMIT|GROUP|$)/i);
        if (whereMatch) {
            const whereClause = whereMatch[1].trim();
            if (whereClause.match(/id\s*=\s*\?/i)) {
                const id = jsParams[0];
                count = table.filter(row => row.id === id).length;
            }
        }
        
        const result = { [columnName]: toQueryValue(count) };
        console.log('[bridgeWasmDb] COUNT query result:', result, 'table:', tableName, 'count:', count);
        return [result];
    }
    
    // SELECT queries
    if (sqlUpper.startsWith('SELECT')) {
        const fromMatch = sql.match(/FROM\s+(\w+)/i);
        if (!fromMatch) return [];
        
        const tableName = fromMatch[1];
        const table = tables[tableName] || [];
        
        // WHERE clause
        const whereMatch = sql.match(/WHERE\s+(.+?)(?:ORDER|LIMIT|$)/i);
        let filtered = [...table];
        
        if (whereMatch) {
            const whereClause = whereMatch[1].trim();
            
            // Simple WHERE id = ? parsing
            if (whereClause.match(/id\s*=\s*\?/i)) {
                const id = jsParams[0];
                filtered = table.filter(row => row.id === id);
            }
            // Add more WHERE parsers as needed
        }
        
        // ORDER BY clause
        const orderMatch = sql.match(/ORDER\s+BY\s+(\w+)(?:\s+(ASC|DESC))?/i);
        if (orderMatch) {
            const orderCol = orderMatch[1];
            const orderDir = (orderMatch[2] || 'ASC').toUpperCase();
            filtered.sort((a, b) => {
                const aVal = a[orderCol];
                const bVal = b[orderCol];
                const cmp = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
                return orderDir === 'DESC' ? -cmp : cmp;
            });
        }
        
        // LIMIT clause
        const limitMatch = sql.match(/LIMIT\s+(\d+)(?:\s+OFFSET\s+(\d+))?/i);
        if (limitMatch) {
            const limit = parseInt(limitMatch[1]);
            const offset = parseInt(limitMatch[2] || 0);
            filtered = filtered.slice(offset, offset + limit);
        }
        
        // Convert to QueryValue format
        return filtered.map(row => {
            const obj = {};
            for (const [key, value] of Object.entries(row)) {
                obj[key] = toQueryValue(value);
            }
            return obj;
        });
    }
    
    return [];
}

/**
 * Execute INSERT/UPDATE/DELETE
 */
function executeModify(sql, params) {
    const sqlUpper = sql.trim().toUpperCase();
    const jsParams = params.map(fromQueryValue);
    
    // INSERT
    if (sqlUpper.startsWith('INSERT')) {
        const intoMatch = sql.match(/INSERT\s+INTO\s+(\w+)/i);
        if (!intoMatch) {
            console.error('[bridgeWasmDb] INSERT: Could not parse table name');
            return 0;
        }
        
        const tableName = intoMatch[1];
        const table = tables[tableName];
        if (!table) {
            console.error('[bridgeWasmDb] INSERT: Table not found:', tableName);
            return 0;
        }
        
        // Extract column names (handle multiline, extra spaces)
        const columnsMatch = sql.match(/\(([^)]+)\)\s+VALUES/i);
        if (!columnsMatch) {
            console.error('[bridgeWasmDb] INSERT: Could not parse columns');
            return 0;
        }
        
        const columns = columnsMatch[1]
            .split(',')
            .map(c => c.trim())
            .filter(c => c.length > 0);
        
        console.log(`[bridgeWasmDb] INSERT into ${tableName}:`, columns.length, 'columns,', jsParams.length, 'params');
        
        if (columns.length !== jsParams.length) {
            console.error('[bridgeWasmDb] INSERT: Column count mismatch!', {
                columns: columns.length,
                params: jsParams.length,
                columnNames: columns
            });
            return 0;
        }
        
        // Create row object
        const row = {};
        columns.forEach((col, i) => {
            row[col] = jsParams[i];
        });
        
        table.push(row);
        console.log(`[bridgeWasmDb] ✅ Inserted into ${tableName}:`, row);
        console.log(`[bridgeWasmDb] Table ${tableName} now has ${table.length} records`);
        return 1;
    }
    
    // UPDATE
    if (sqlUpper.startsWith('UPDATE')) {
        const tableMatch = sql.match(/UPDATE\s+(\w+)/i);
        if (!tableMatch) return 0;
        
        const tableName = tableMatch[1];
        const table = tables[tableName];
        if (!table) return 0;
        
        // Simple WHERE id = ? parsing
        const whereMatch = sql.match(/WHERE\s+id\s*=\s*\?/i);
        if (whereMatch) {
            const id = jsParams[jsParams.length - 1]; // Last param is usually ID
            const index = table.findIndex(row => row.id === id);
            if (index !== -1) {
                // Update logic would go here
                return 1;
            }
        }
        return 0;
    }
    
    // DELETE
    if (sqlUpper.startsWith('DELETE')) {
        const fromMatch = sql.match(/DELETE\s+FROM\s+(\w+)/i);
        if (!fromMatch) return 0;
        
        const tableName = fromMatch[1];
        const table = tables[tableName];
        if (!table) return 0;
        
        const whereMatch = sql.match(/WHERE\s+id\s*=\s*\?/i);
        if (whereMatch) {
            const id = jsParams[0];
            const index = table.findIndex(row => row.id === id);
            if (index !== -1) {
                table.splice(index, 1);
                return 1;
            }
        }
        return 0;
    }
    
    return 0;
}

// =============================================================================
// Global bridgeWasmDb API
// =============================================================================

window.bridgeWasmDb = {
    /**
     * Initialize database connection
     */
    async init(databaseUrl) {
        console.log('[bridgeWasmDb] Initializing database:', databaseUrl);
        console.log('[bridgeWasmDb] Using native IndexedDB - no external dependencies!');
        
        // Load existing data from IndexedDB
        await loadFromIndexedDB();
        
        db = true; // Mark as initialized
    },

    /**
     * Execute SELECT query and return rows
     */
    async query(handle, sql, params) {
        try {
            console.log('[bridgeWasmDb] QUERY:', sql.substring(0, 100), 'params:', params.length);
            const rows = parseSQL(sql, params);
            console.log('[bridgeWasmDb] QUERY returned:', rows.length, 'rows', rows[0] ? Object.keys(rows[0]) : 'no rows');
            return rows;
        } catch (error) {
            console.error('[bridgeWasmDb] Query failed:', error, { sql, params });
            throw error;
        }
    },

    /**
     * Execute INSERT/UPDATE/DELETE and return affected rows
     */
    async execute(handle, sql, params) {
        try {
            const affected = executeModify(sql, params);
            
            // Auto-save to IndexedDB after modifications
            await this.autoSave();
            
            return affected;
        } catch (error) {
            console.error('[bridgeWasmDb] Execute failed:', error, { sql, params });
            throw error;
        }
    },

    /**
     * Initialize database (run migrations)
     */
    async initialize(handle) {
        console.log('[bridgeWasmDb] Initializing database schema...');
        
        // Load migration SQL files
        try {
            const migration001 = await fetch('./migrations/001_initial_schema.sql').then(r => r.text());
            const migration002 = await fetch('./migrations/002_add_model_fields.sql').then(r => r.text());
            
            // Check if migrations already applied
            const version = await this.getSchemaVersion();
            
            if (version < 1) {
                console.log('[bridgeWasmDb] Applying migration 001_initial_schema');
                await this.applyMigration(null, 1, migration001);
            }
            
            if (version < 2) {
                console.log('[bridgeWasmDb] Applying migration 002_add_model_fields');
                await this.applyMigration(null, 2, migration002);
            }
            
            console.log('[bridgeWasmDb] ✅ Schema initialized successfully');
        } catch (error) {
            console.error('[bridgeWasmDb] Failed to initialize schema:', error);
            throw error;
        }
    },

    /**
     * Health check
     */
    async healthCheck(handle) {
        return db !== null;
    },

    /**
     * Close database
     */
    async close(handle) {
        console.log('[bridgeWasmDb] Closing database...');
        if (db) {
            await saveToIndexedDB();
            db = null;
        }
    },

    /**
     * Get last inserted row ID
     */
    async lastInsertRowid(handle) {
        // Return the last inserted ID (simple approximation)
        return 0;
    },

    /**
     * Begin transaction
     */
    async beginTransaction(handle) {
        const txId = nextTxId++;
        // Store snapshot for rollback
        transactions.set(txId, {
            artists: JSON.parse(JSON.stringify(tables.artists)),
            albums: JSON.parse(JSON.stringify(tables.albums)),
            tracks: JSON.parse(JSON.stringify(tables.tracks)),
            playlists: JSON.parse(JSON.stringify(tables.playlists)),
            playlist_tracks: JSON.parse(JSON.stringify(tables.playlist_tracks))
        });
        console.log('[bridgeWasmDb] Transaction started:', txId);
        return txId;
    },

    /**
     * Commit transaction
     */
    async commitTransaction(handle, txId) {
        if (!transactions.has(txId)) {
            throw new Error(`Transaction ${txId} not found`);
        }
        transactions.delete(txId);
        await this.autoSave();
        console.log('[bridgeWasmDb] Transaction committed:', txId);
    },

    /**
     * Rollback transaction
     */
    async rollbackTransaction(handle, txId) {
        if (!transactions.has(txId)) {
            throw new Error(`Transaction ${txId} not found`);
        }
        const snapshot = transactions.get(txId);
        Object.assign(tables, snapshot);
        transactions.delete(txId);
        console.log('[bridgeWasmDb] Transaction rolled back:', txId);
    },

    /**
     * Get schema version
     */
    async getSchemaVersion(handle) {
        if (!tables._migrations || tables._migrations.length === 0) {
            return 0;
        }
        // Return the highest migration version
        return Math.max(...tables._migrations.map(m => m.version));
    },

    /**
     * Apply migration
     */
    async applyMigration(handle, version, sql) {
        console.log(`[bridgeWasmDb] Applying migration ${version}...`);
        
        try {
            // Parse CREATE TABLE statements to understand schema
            const createTableRegex = /CREATE\s+TABLE\s+(\w+)\s*\(([^;]+)\)/gi;
            let match;
            
            while ((match = createTableRegex.exec(sql)) !== null) {
                const tableName = match[1];
                const columns = match[2];
                
                // Parse column definitions
                schema[tableName] = parseTableColumns(columns);
                
                console.log(`[bridgeWasmDb] Parsed table: ${tableName} with ${schema[tableName].length} columns`);
                
                // Initialize table if it doesn't exist
                if (!tables[tableName]) {
                    tables[tableName] = [];
                }
            }
            
            // Record migration as applied
            tables._migrations.push({
                version: version,
                applied_at: Date.now(),
                description: `Migration ${version}`
            });
            
            await this.autoSave();
            console.log(`[bridgeWasmDb] ✅ Migration ${version} applied successfully`);
        } catch (error) {
            console.error(`[bridgeWasmDb] Failed to apply migration ${version}:`, error);
            throw error;
        }
    },

    /**
     * Check if migration is applied
     */
    async isMigrationApplied(handle, version) {
        const currentVersion = await this.getSchemaVersion();
        return currentVersion >= version;
    },

    /**
     * Auto-save to IndexedDB (debounced)
     */
    autoSave: (() => {
        let timeout;
        return async function() {
            clearTimeout(timeout);
            timeout = setTimeout(async () => {
                if (db) {
                    await saveToIndexedDB();
                    console.log('[bridgeWasmDb] Auto-saved to IndexedDB');
                }
            }, 1000); // Save 1 second after last modification
        };
    })(),

    /**
     * Manual save to IndexedDB
     */
    async save() {
        if (db) {
            await saveToIndexedDB();
            console.log('[bridgeWasmDb] Manually saved to IndexedDB');
        }
    },

    /**
     * Get database statistics
     */
    async getStatistics(handle) {
        const stats = {
            totalConnections: 1,
            idleConnections: 0,
            activeConnections: 1,
            databaseSizeBytes: JSON.stringify(tables).length,
            cachedStatements: 0
        };
        return stats;
    }
};

console.log('✅ bridgeWasmDb initialized (native IndexedDB, zero dependencies)');
