//! Settings Storage using SQLite

use async_trait::async_trait;
use bridge_traits::{
    error::{BridgeError, Result},
    storage::{SettingsStore, SettingsTransaction},
};
use sqlx::{sqlite::SqlitePool, Row};
use std::path::PathBuf;
use tracing::{debug, error};

/// SQLite-backed settings store implementation
///
/// Provides persistent key-value storage using SQLite:
/// - Type-safe value storage
/// - Transactional updates
/// - Async operations
pub struct SqliteSettingsStore {
    pool: SqlitePool,
}

impl SqliteSettingsStore {
    /// Create a new settings store with the given database path
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(BridgeError::Io)?;
        }

        // Convert path to string, replacing backslashes with forward slashes for SQLite URL
        let path_str = db_path.to_string_lossy().replace('\\', "/");
        let db_url = format!("sqlite://{}", path_str);

        let pool = SqlitePool::connect(&db_url)
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to connect to DB: {}", e)))?;

        // Create settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                value_type TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| BridgeError::OperationFailed(format!("Failed to create table: {}", e)))?;

        debug!(path = ?db_path, "Initialized settings store");

        Ok(Self { pool })
    }

    /// Create an in-memory settings store (for testing)
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to connect to DB: {}", e)))?;

        // Create settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                value_type TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| BridgeError::OperationFailed(format!("Failed to create table: {}", e)))?;

        Ok(Self { pool })
    }

    /// Get the current Unix timestamp
    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Set a value with type information
    async fn set_value(&self, key: &str, value: &str, value_type: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, value_type, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                value_type = excluded.value_type,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(value_type)
        .bind(Self::now())
        .execute(&self.pool)
        .await
        .map_err(|e| BridgeError::OperationFailed(format!("Failed to set setting: {}", e)))?;

        debug!(key = key, value_type = value_type, "Stored setting");
        Ok(())
    }

    /// Get a value and verify its type
    async fn get_value(&self, key: &str, expected_type: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value, value_type FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to get setting: {}", e)))?;

        match row {
            Some(row) => {
                let value: String = row.get(0);
                let value_type: String = row.get(1);

                if value_type != expected_type {
                    error!(
                        key = key,
                        expected = expected_type,
                        actual = value_type,
                        "Type mismatch"
                    );
                    return Err(BridgeError::OperationFailed(format!(
                        "Type mismatch: expected {}, got {}",
                        expected_type, value_type
                    )));
                }

                debug!(key = key, value_type = value_type, "Retrieved setting");
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

#[async_trait]
impl SettingsStore for SqliteSettingsStore {
    async fn set_string(&self, key: &str, value: &str) -> Result<()> {
        self.set_value(key, value, "string").await
    }

    async fn get_string(&self, key: &str) -> Result<Option<String>> {
        self.get_value(key, "string").await
    }

    async fn set_bool(&self, key: &str, value: bool) -> Result<()> {
        self.set_value(key, &value.to_string(), "bool").await
    }

    async fn get_bool(&self, key: &str) -> Result<Option<bool>> {
        match self.get_value(key, "bool").await? {
            Some(s) => Ok(Some(s.parse().map_err(|e| {
                BridgeError::OperationFailed(format!("Parse error: {}", e))
            })?)),
            None => Ok(None),
        }
    }

    async fn set_i64(&self, key: &str, value: i64) -> Result<()> {
        self.set_value(key, &value.to_string(), "i64").await
    }

    async fn get_i64(&self, key: &str) -> Result<Option<i64>> {
        match self.get_value(key, "i64").await? {
            Some(s) => Ok(Some(s.parse().map_err(|e| {
                BridgeError::OperationFailed(format!("Parse error: {}", e))
            })?)),
            None => Ok(None),
        }
    }

    async fn set_f64(&self, key: &str, value: f64) -> Result<()> {
        self.set_value(key, &value.to_string(), "f64").await
    }

    async fn get_f64(&self, key: &str) -> Result<Option<f64>> {
        match self.get_value(key, "f64").await? {
            Some(s) => Ok(Some(s.parse().map_err(|e| {
                BridgeError::OperationFailed(format!("Parse error: {}", e))
            })?)),
            None => Ok(None),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                BridgeError::OperationFailed(format!("Failed to delete setting: {}", e))
            })?;

        debug!(key = key, "Deleted setting");
        Ok(())
    }

    async fn has_key(&self, key: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to check key: {}", e)))?;

        Ok(row.is_some())
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT key FROM settings ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to list keys: {}", e)))?;

        let keys = rows.into_iter().map(|row| row.get(0)).collect();
        Ok(keys)
    }

    async fn clear_all(&self) -> Result<()> {
        sqlx::query("DELETE FROM settings")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                BridgeError::OperationFailed(format!("Failed to clear settings: {}", e))
            })?;

        debug!("Cleared all settings");
        Ok(())
    }

    async fn begin_transaction(&self) -> Result<Box<dyn SettingsTransaction + Send>> {
        let tx = self.pool.begin().await.map_err(|e| {
            BridgeError::OperationFailed(format!("Failed to begin transaction: {}", e))
        })?;

        Ok(Box::new(SqliteSettingsTransaction { tx: Some(tx) }))
    }
}

/// SQLite settings transaction
struct SqliteSettingsTransaction {
    tx: Option<sqlx::Transaction<'static, sqlx::Sqlite>>,
}

#[async_trait]
impl SettingsTransaction for SqliteSettingsTransaction {
    async fn set_string(&mut self, key: &str, value: &str) -> Result<()> {
        let tx = self.tx.as_mut().ok_or_else(|| {
            BridgeError::OperationFailed("Transaction already committed".to_string())
        })?;

        sqlx::query(
            r#"
            INSERT INTO settings (key, value, value_type, updated_at)
            VALUES (?, ?, 'string', ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                value_type = excluded.value_type,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(SqliteSettingsStore::now())
        .execute(&mut **tx)
        .await
        .map_err(|e| BridgeError::OperationFailed(format!("Failed to set setting: {}", e)))?;

        Ok(())
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let tx = self.tx.take().ok_or_else(|| {
            BridgeError::OperationFailed("Transaction already committed".to_string())
        })?;

        tx.commit()
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to commit: {}", e)))?;

        debug!("Committed transaction");
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<()> {
        let tx = self.tx.take().ok_or_else(|| {
            BridgeError::OperationFailed("Transaction already committed".to_string())
        })?;

        tx.rollback()
            .await
            .map_err(|e| BridgeError::OperationFailed(format!("Failed to rollback: {}", e)))?;

        debug!("Rolled back transaction");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_settings_store_creation() {
        let _store = SqliteSettingsStore::in_memory().await.unwrap();
        // Just verify it constructs
    }

    #[tokio::test]
    async fn test_string_operations() {
        let store = SqliteSettingsStore::in_memory().await.unwrap();

        store.set_string("test_key", "test_value").await.unwrap();
        let value = store.get_string("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        store.delete("test_key").await.unwrap();
        let value = store.get_string("test_key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_typed_operations() {
        let store = SqliteSettingsStore::in_memory().await.unwrap();

        // Bool
        store.set_bool("bool_key", true).await.unwrap();
        assert_eq!(store.get_bool("bool_key").await.unwrap(), Some(true));

        // i64
        store.set_i64("i64_key", 42).await.unwrap();
        assert_eq!(store.get_i64("i64_key").await.unwrap(), Some(42));

        // f64
        store.set_f64("f64_key", 2.5).await.unwrap();
        assert_eq!(store.get_f64("f64_key").await.unwrap(), Some(2.5));
    }

    #[tokio::test]
    async fn test_list_keys() {
        let store = SqliteSettingsStore::in_memory().await.unwrap();

        store.set_string("key1", "value1").await.unwrap();
        store.set_string("key2", "value2").await.unwrap();

        let keys = store.list_keys().await.unwrap();
        assert_eq!(keys, vec!["key1", "key2"]);
    }
}
