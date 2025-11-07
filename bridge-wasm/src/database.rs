//! WebAssembly Database Adapter implementation.
//!
//! This module provides a [`DatabaseAdapter`] implementation that bridges to a
//! host-provided JavaScript database runtime (e.g., `sql.js` backed by
//! IndexedDB). The actual SQL engine runs on the JavaScript side; this Rust
//! adapter focuses on type conversion, error handling, and trait compliance so
//! the core library can remain platform-agnostic.
//!
//! # Host Requirements
//!
//! The JavaScript environment must expose a global `bridgeWasmDb` namespace
//! with the following async functions (returning `Promise`):
//!
//! - `init(config) -> handle`
//! - `close(handle)`
//! - `initialize(handle)`
//! - `healthCheck(handle)`
//! - `query(handle, sql, params) -> rows`
//! - `execute(handle, sql, params) -> rowsAffected`
//! - `beginTransaction(handle) -> transactionId`
//! - `commitTransaction(handle, transactionId)`
//! - `rollbackTransaction(handle, transactionId)`
//! - `queryInTransaction(handle, transactionId, sql, params) -> rows`
//! - `executeInTransaction(handle, transactionId, sql, params) -> rowsAffected`
//! - `executeBatch(handle, statements)`
//! - `getSchemaVersion(handle) -> version`
//! - `applyMigration(handle, version, sql)`
//! - `isMigrationApplied(handle, version) -> bool`
//! - `lastInsertRowId(handle) -> id`
//! - `getStatistics(handle) -> { ... }`
//!
//! The bridge serializes Rust data using `serde_wasm_bindgen`, so the JavaScript
//! implementation must understand the serialized structures defined by
//! `bridge-traits`.

use bridge_traits::database::{
    DatabaseAdapter, DatabaseConfig, DatabaseStatistics, QueryRow, QueryValue, TransactionId,
};
use bridge_traits::error::{BridgeError, Result as BridgeResult};
use js_sys::Promise;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_wasm_bindgen::{from_value, to_value};
use std::io;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::error::{WasmError, WasmResult};

/// WASM implementation of the [`DatabaseAdapter`] trait.
///
/// This adapter delegates all database work to JavaScript while keeping the
/// Rust-side API identical to the native `SqliteAdapter`.
pub struct WasmDbAdapter {
    handle: JsValue,
    #[allow(dead_code)]
    config: DatabaseConfig,
}

impl WasmDbAdapter {
    /// Create a new adapter instance.
    pub async fn new(config: DatabaseConfig) -> WasmResult<Self> {
        let js_config = JsAdapterConfig::from(&config);
        let handle = call_js_promise(init_database(&to_js_value(&js_config)?)).await?;
        Ok(Self { handle, config })
    }

    fn params_to_js(params: &[QueryValue]) -> WasmResult<JsValue> {
        to_js_value(params)
    }

    fn rows_from_js(value: JsValue) -> WasmResult<Vec<QueryRow>> {
        from_value(value).map_err(serde_to_wasm_error)
    }

    fn stats_from_js(value: JsValue) -> WasmResult<DatabaseStatistics> {
        from_value::<JsDatabaseStatistics>(value)
            .map(DatabaseStatistics::from)
            .map_err(serde_to_wasm_error)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DatabaseAdapter for WasmDbAdapter {
    async fn initialize(&mut self) -> BridgeResult<()> {
        call_bridge_promise(db_initialize(&self.handle)).await?;
        Ok(())
    }

    async fn health_check(&self) -> BridgeResult<()> {
        call_bridge_promise(db_health_check(&self.handle)).await?;
        Ok(())
    }

    async fn close(&mut self) -> BridgeResult<()> {
        call_bridge_promise(db_close(&self.handle)).await?;
        Ok(())
    }

    async fn query(&self, query: &str, params: &[QueryValue]) -> BridgeResult<Vec<QueryRow>> {
        let js_rows =
            call_bridge_promise(db_query(&self.handle, query, &Self::params_to_js(params)?))
                .await?;
        Ok(Self::rows_from_js(js_rows)?)
    }

    async fn execute(&self, statement: &str, params: &[QueryValue]) -> BridgeResult<u64> {
        let affected = call_bridge_promise(db_execute(
            &self.handle,
            statement,
            &Self::params_to_js(params)?,
        ))
        .await?;
        Ok(js_value_to_u64(affected)?)
    }

    async fn query_one_optional(
        &self,
        query: &str,
        params: &[QueryValue],
    ) -> BridgeResult<Option<QueryRow>> {
        let mut rows = self.query(query, params).await?;
        Ok(rows.pop())
    }

    async fn query_one(&self, query: &str, params: &[QueryValue]) -> BridgeResult<QueryRow> {
        let mut rows = self.query(query, params).await?;
        rows.pop()
            .ok_or_else(|| BridgeError::DatabaseError("Query returned no rows".into()))
    }

    async fn begin_transaction(&self) -> BridgeResult<TransactionId> {
        let id = call_bridge_promise(db_begin_transaction(&self.handle)).await?;
        Ok(TransactionId(js_value_to_u64(id)?))
    }

    async fn commit_transaction(&self, transaction_id: TransactionId) -> BridgeResult<()> {
        call_bridge_promise(db_commit_transaction(&self.handle, transaction_id.0)).await?;
        Ok(())
    }

    async fn rollback_transaction(&self, transaction_id: TransactionId) -> BridgeResult<()> {
        call_bridge_promise(db_rollback_transaction(&self.handle, transaction_id.0)).await?;
        Ok(())
    }

    async fn query_in_transaction(
        &self,
        transaction_id: TransactionId,
        query: &str,
        params: &[QueryValue],
    ) -> BridgeResult<Vec<QueryRow>> {
        let js_rows = call_bridge_promise(db_query_in_transaction(
            &self.handle,
            transaction_id.0,
            query,
            &Self::params_to_js(params)?,
        ))
        .await?;
        Ok(Self::rows_from_js(js_rows)?)
    }

    async fn execute_in_transaction(
        &self,
        transaction_id: TransactionId,
        statement: &str,
        params: &[QueryValue],
    ) -> BridgeResult<u64> {
        let affected = call_bridge_promise(db_execute_in_transaction(
            &self.handle,
            transaction_id.0,
            statement,
            &Self::params_to_js(params)?,
        ))
        .await?;
        Ok(js_value_to_u64(affected)?)
    }

    async fn execute_batch(&self, statements: &[(&str, &[QueryValue])]) -> BridgeResult<Vec<u64>> {
        let serialized: Vec<BatchStatement> = statements
            .iter()
            .map(|(sql, params)| BatchStatement {
                sql: sql.to_string(),
                params: params.to_vec(),
            })
            .collect();
        let js_value = to_js_value(&serialized)?;
        let counts = call_bridge_promise(db_execute_batch(&self.handle, &js_value)).await?;
        from_value(counts).map_err(|e| BridgeError::from(serde_to_wasm_error(e)))
    }

    async fn get_schema_version(&self) -> BridgeResult<i64> {
        let version = call_bridge_promise(db_get_schema_version(&self.handle)).await?;
        js_value_to_i64(version)
    }

    async fn apply_migration(&self, version: i64, up_sql: &str) -> BridgeResult<()> {
        call_bridge_promise(db_apply_migration(&self.handle, version, up_sql)).await?;
        Ok(())
    }

    async fn is_migration_applied(&self, version: i64) -> BridgeResult<bool> {
        let flag = call_bridge_promise(db_is_migration_applied(&self.handle, version)).await?;
        js_value_to_bool(flag)
    }

    async fn last_insert_rowid(&self) -> BridgeResult<i64> {
        let id = call_bridge_promise(db_last_insert_rowid(&self.handle)).await?;
        js_value_to_i64(id)
    }

    async fn get_statistics(&self) -> BridgeResult<DatabaseStatistics> {
        let stats = call_bridge_promise(db_get_statistics(&self.handle)).await?;
        Ok(Self::stats_from_js(stats)?)
    }
}

/// Serializable subset of [`DatabaseConfig`] forwarded to JavaScript.
#[derive(Debug, Serialize)]
struct JsAdapterConfig<'a> {
    database_url: &'a str,
    min_connections: u32,
    max_connections: u32,
    acquire_timeout_secs: u64,
    enable_cache: bool,
    cache_capacity: usize,
}

impl<'a> From<&'a DatabaseConfig> for JsAdapterConfig<'a> {
    fn from(value: &'a DatabaseConfig) -> Self {
        Self {
            database_url: &value.database_url,
            min_connections: value.min_connections,
            max_connections: value.max_connections,
            acquire_timeout_secs: value.acquire_timeout_secs,
            enable_cache: value.enable_cache,
            cache_capacity: value.cache_capacity,
        }
    }
}

#[derive(Debug, Serialize)]
struct BatchStatement {
    sql: String,
    params: Vec<QueryValue>,
}

#[derive(Debug, Deserialize)]
struct JsDatabaseStatistics {
    total_connections: u32,
    idle_connections: u32,
    active_connections: u32,
    database_size_bytes: Option<u64>,
    cached_statements: usize,
}

impl From<JsDatabaseStatistics> for DatabaseStatistics {
    fn from(value: JsDatabaseStatistics) -> Self {
        DatabaseStatistics {
            total_connections: value.total_connections,
            idle_connections: value.idle_connections,
            active_connections: value.active_connections,
            database_size_bytes: value.database_size_bytes,
            cached_statements: value.cached_statements,
        }
    }
}

fn to_js_value<T: Serialize + ?Sized>(value: &T) -> WasmResult<JsValue> {
    to_value(value).map_err(serde_to_wasm_error)
}

async fn call_js_promise(promise: Promise) -> WasmResult<JsValue> {
    JsFuture::from(promise).await.map_err(WasmError::from)
}

async fn call_bridge_promise(promise: Promise) -> BridgeResult<JsValue> {
    call_js_promise(promise).await.map_err(BridgeError::from)
}

fn js_value_to_u64(value: JsValue) -> BridgeResult<u64> {
    value
        .as_f64()
        .map(|v| v as u64)
        .ok_or_else(|| BridgeError::DatabaseError("Expected number".into()))
}

fn js_value_to_i64(value: JsValue) -> BridgeResult<i64> {
    value
        .as_f64()
        .map(|v| v as i64)
        .ok_or_else(|| BridgeError::DatabaseError("Expected number".into()))
}

fn js_value_to_bool(value: JsValue) -> BridgeResult<bool> {
    value
        .as_bool()
        .ok_or_else(|| BridgeError::DatabaseError("Expected boolean".into()))
}

fn serde_to_wasm_error(err: serde_wasm_bindgen::Error) -> WasmError {
    let io_err = io::Error::new(io::ErrorKind::Other, err.to_string());
    WasmError::Serialization(serde_json::Error::io(io_err))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = init)]
    fn init_database(config: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = close)]
    fn db_close(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = initialize)]
    fn db_initialize(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = healthCheck)]
    fn db_health_check(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = query)]
    fn db_query(handle: &JsValue, sql: &str, params: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = execute)]
    fn db_execute(handle: &JsValue, sql: &str, params: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = beginTransaction)]
    fn db_begin_transaction(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = commitTransaction)]
    fn db_commit_transaction(handle: &JsValue, transaction_id: u64) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = rollbackTransaction)]
    fn db_rollback_transaction(handle: &JsValue, transaction_id: u64) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = queryInTransaction)]
    fn db_query_in_transaction(
        handle: &JsValue,
        transaction_id: u64,
        sql: &str,
        params: &JsValue,
    ) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = executeInTransaction)]
    fn db_execute_in_transaction(
        handle: &JsValue,
        transaction_id: u64,
        sql: &str,
        params: &JsValue,
    ) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = executeBatch)]
    fn db_execute_batch(handle: &JsValue, statements: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = getSchemaVersion)]
    fn db_get_schema_version(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = applyMigration)]
    fn db_apply_migration(handle: &JsValue, version: i64, sql: &str) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = isMigrationApplied)]
    fn db_is_migration_applied(handle: &JsValue, version: i64) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = lastInsertRowId)]
    fn db_last_insert_rowid(handle: &JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = bridgeWasmDb, js_name = getStatistics)]
    fn db_get_statistics(handle: &JsValue) -> Promise;
}
