//! WebAssembly implementations of the secure and settings storage bridges.
//!
//! Secrets are encrypted using AES-256-GCM with a randomly generated master
//! key that is itself persisted (base64 encoded) in `localStorage`. Settings
//! leverage the same storage backend but store plaintext values since they are
//! not considered sensitive. Namespaces ensure multiple host shells can coexist
//! without clobbering each other's data.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use bridge_traits::{
    error::{BridgeError, Result as BridgeResult},
    storage::{SecureStore, SettingsStore, SettingsTransaction},
};
use rand::{rngs::OsRng, RngCore};
use std::collections::HashMap;
use wasm_bindgen::{JsCast, JsValue};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

const SECURE_MASTER_KEY_SUFFIX: &str = "secure-master-key";
const SECURE_PREFIX: &str = "secure";
const SETTINGS_PREFIX: &str = "settings";

fn js_error(context: &str, err: JsValue) -> BridgeError {
    let message = if err.is_string() {
        err.as_string().unwrap_or_default()
    } else if let Some(js_err) = err.dyn_ref::<js_sys::Error>() {
        js_err.message().into()
    } else {
        format!("{err:?}")
    };
    BridgeError::OperationFailed(format!("wasm storage {context}: {message}"))
}

fn local_storage() -> BridgeResult<web_sys::Storage> {
    let window = web_sys::window().ok_or_else(|| BridgeError::NotAvailable("window".into()))?;
    window
        .local_storage()
        .map_err(|err| js_error("localStorage", err))?
        .ok_or_else(|| BridgeError::NotAvailable("localStorage".into()))
}

fn namespaced_prefix(namespace: &str, kind: &str) -> String {
    format!("{namespace}::{kind}::")
}

fn scoped_key(namespace: &str, kind: &str, key: &str) -> String {
    format!("{namespace}::{kind}::{key}")
}

#[derive(Clone)]
/// AES-GCM backed secure storage implementation for browsers.
pub struct WasmSecureStore {
    storage: web_sys::Storage,
    namespace: String,
    master_key: [u8; 32],
}

impl WasmSecureStore {
    /// Construct a new secure store scoped to the provided namespace.
    pub fn new(namespace: impl Into<String>) -> BridgeResult<Self> {
        let namespace = namespace.into();
        let storage = local_storage()?;
        let master_key = load_or_create_master_key(&storage, &namespace)?;
        Ok(Self {
            storage,
            namespace,
            master_key,
        })
    }

    fn cipher(&self) -> BridgeResult<Aes256Gcm> {
        Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|err| BridgeError::OperationFailed(format!("cipher init failed: {err}")))
    }

    fn key_for(&self, key: &str) -> String {
        scoped_key(&self.namespace, SECURE_PREFIX, key)
    }

    fn key_prefix(&self) -> String {
        namespaced_prefix(&self.namespace, SECURE_PREFIX)
    }
}

#[async_trait(?Send)]
impl SecureStore for WasmSecureStore {
    async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> {
        let cipher = self.cipher()?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, value)
            .map_err(|err| BridgeError::OperationFailed(format!("encrypt secret: {err}")))?;

        let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);

        let encoded = BASE64.encode(payload);
        self.storage
            .set_item(&self.key_for(key), &encoded)
            .map_err(|err| js_error("set_item", err))
    }

    async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> {
        let stored = match self
            .storage
            .get_item(&self.key_for(key))
            .map_err(|err| js_error("get_item", err))?
        {
            Some(value) => value,
            None => return Ok(None),
        };

        let data = BASE64
            .decode(stored)
            .map_err(|err| BridgeError::OperationFailed(format!("decode secret: {err}")))?;

        if data.len() <= 12 {
            return Err(BridgeError::OperationFailed(
                "stored secret payload too small".into(),
            ));
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let mut nonce_array = [0u8; 12];
        nonce_array.copy_from_slice(nonce_bytes);
        let nonce = Nonce::from(nonce_array);
        let cipher = self.cipher()?;
        let plaintext = cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|err| BridgeError::OperationFailed(format!("decrypt secret: {err}")))?;

        Ok(Some(plaintext))
    }

    async fn delete_secret(&self, key: &str) -> BridgeResult<()> {
        self.storage
            .remove_item(&self.key_for(key))
            .map_err(|err| js_error("remove_item", err))
    }

    async fn clear_all(&self) -> BridgeResult<()> {
        let keys = self.list_keys().await?;
        for key in keys {
            self.storage
                .remove_item(&self.key_for(&key))
                .map_err(|err| js_error("clear secret", err))?;
        }
        Ok(())
    }

    async fn list_keys(&self) -> BridgeResult<Vec<String>> {
        list_prefixed_keys(&self.storage, &self.key_prefix())
    }
}

fn load_or_create_master_key(
    storage: &web_sys::Storage,
    namespace: &str,
) -> BridgeResult<[u8; 32]> {
    let key_name = scoped_key(namespace, SECURE_PREFIX, SECURE_MASTER_KEY_SUFFIX);

    if let Some(existing) = storage
        .get_item(&key_name)
        .map_err(|err| js_error("get master key", err))?
    {
        let mut key = [0u8; 32];
        let decoded = BASE64
            .decode(existing)
            .map_err(|err| BridgeError::OperationFailed(format!("decode master key: {err}")))?;
        if decoded.len() != 32 {
            return Err(BridgeError::OperationFailed(
                "master key has invalid length".into(),
            ));
        }
        key.copy_from_slice(&decoded);
        return Ok(key);
    }

    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let encoded = BASE64.encode(&key);
    storage
        .set_item(&key_name, &encoded)
        .map_err(|err| js_error("store master key", err))?;
    Ok(key)
}

#[derive(Clone)]
/// Browser-backed settings store (plain-text key/value pairs).
pub struct WasmSettingsStore {
    storage: web_sys::Storage,
    namespace: String,
}

impl WasmSettingsStore {
    /// Construct a new settings store scoped to the provided namespace.
    pub fn new(namespace: impl Into<String>) -> BridgeResult<Self> {
        Ok(Self {
            storage: local_storage()?,
            namespace: namespace.into(),
        })
    }

    fn key_for(&self, key: &str) -> String {
        scoped_key(&self.namespace, SETTINGS_PREFIX, key)
    }

    fn prefix(&self) -> String {
        namespaced_prefix(&self.namespace, SETTINGS_PREFIX)
    }
}

#[async_trait(?Send)]
impl SettingsStore for WasmSettingsStore {
    async fn set_string(&self, key: &str, value: &str) -> BridgeResult<()> {
        self.storage
            .set_item(&self.key_for(key), value)
            .map_err(|err| js_error("set setting", err))
    }

    async fn get_string(&self, key: &str) -> BridgeResult<Option<String>> {
        self.storage
            .get_item(&self.key_for(key))
            .map_err(|err| js_error("get setting", err))
    }

    async fn set_bool(&self, key: &str, value: bool) -> BridgeResult<()> {
        self.set_string(key, if value { "true" } else { "false" })
            .await
    }

    async fn get_bool(&self, key: &str) -> BridgeResult<Option<bool>> {
        Ok(match self.get_string(key).await? {
            Some(value) => Some(matches!(value.as_str(), "true" | "1")),
            None => None,
        })
    }

    async fn set_i64(&self, key: &str, value: i64) -> BridgeResult<()> {
        self.set_string(key, &value.to_string()).await
    }

    async fn get_i64(&self, key: &str) -> BridgeResult<Option<i64>> {
        match self.get_string(key).await? {
            Some(value) => value
                .parse::<i64>()
                .map(Some)
                .map_err(|err| BridgeError::OperationFailed(format!("parse i64: {err}"))),
            None => Ok(None),
        }
    }

    async fn set_f64(&self, key: &str, value: f64) -> BridgeResult<()> {
        self.set_string(key, &value.to_string()).await
    }

    async fn get_f64(&self, key: &str) -> BridgeResult<Option<f64>> {
        match self.get_string(key).await? {
            Some(value) => value
                .parse::<f64>()
                .map(Some)
                .map_err(|err| BridgeError::OperationFailed(format!("parse f64: {err}"))),
            None => Ok(None),
        }
    }

    async fn delete(&self, key: &str) -> BridgeResult<()> {
        self.storage
            .remove_item(&self.key_for(key))
            .map_err(|err| js_error("remove setting", err))
    }

    async fn has_key(&self, key: &str) -> BridgeResult<bool> {
        Ok(self.get_string(key).await?.is_some())
    }

    async fn list_keys(&self) -> BridgeResult<Vec<String>> {
        list_prefixed_keys(&self.storage, &self.prefix())
    }

    async fn clear_all(&self) -> BridgeResult<()> {
        let keys = self.list_keys().await?;
        for key in keys {
            self.storage
                .remove_item(&self.key_for(&key))
                .map_err(|err| js_error("clear setting", err))?;
        }
        Ok(())
    }

    async fn begin_transaction(&self) -> BridgeResult<Box<dyn SettingsTransaction>> {
        Ok(Box::new(LocalSettingsTransaction::new(
            self.storage.clone(),
            self.namespace.clone(),
        )))
    }
}

fn list_prefixed_keys(storage: &web_sys::Storage, prefix: &str) -> BridgeResult<Vec<String>> {
    let len = storage
        .length()
        .map_err(|err| js_error("storage length", err))?;
    let mut keys = Vec::new();
    for idx in 0..len {
        if let Some(entry) = storage
            .key(idx)
            .map_err(|err| js_error("storage key", err))?
        {
            if entry.starts_with(prefix) {
                keys.push(entry[prefix.len()..].to_string());
            }
        }
    }
    Ok(keys)
}

struct LocalSettingsTransaction {
    storage: web_sys::Storage,
    namespace: String,
    staged: HashMap<String, String>,
    committed: bool,
}

impl LocalSettingsTransaction {
    fn new(storage: web_sys::Storage, namespace: String) -> Self {
        Self {
            storage,
            namespace,
            staged: HashMap::new(),
            committed: false,
        }
    }

    fn key_for(&self, key: &str) -> String {
        scoped_key(&self.namespace, SETTINGS_PREFIX, key)
    }
}

#[async_trait(?Send)]
impl SettingsTransaction for LocalSettingsTransaction {
    async fn set_string(&mut self, key: &str, value: &str) -> BridgeResult<()> {
        self.staged.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn commit(mut self: Box<Self>) -> BridgeResult<()> {
        if self.committed {
            return Ok(());
        }
        for (key, value) in self.staged.iter() {
            self.storage
                .set_item(&self.key_for(key), value)
                .map_err(|err| js_error("txn set_item", err))?;
        }
        self.committed = true;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> BridgeResult<()> {
        self.committed = true;
        Ok(())
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use bridge_traits::storage::{SecureStore, SettingsStore};
    use wasm_bindgen_test::*;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    fn unique_namespace(prefix: &str) -> String {
        format!("{prefix}-{}", js_sys::Date::now())
    }

    #[wasm_bindgen_test]
    async fn secure_store_roundtrip() {
        console_error_panic_hook::set_once();
        let ns = unique_namespace("secure");
        let store = WasmSecureStore::new(ns).expect("secure store init");
        store
            .set_secret("token", b"super-secret")
            .await
            .expect("set secret");
        let value = store
            .get_secret("token")
            .await
            .expect("get secret")
            .expect("value present");
        assert_eq!(value, b"super-secret");
        assert!(store
            .list_keys()
            .await
            .expect("list keys")
            .contains(&"token".to_string()));
        store.delete_secret("token").await.expect("delete secret");
        assert!(store
            .get_secret("token")
            .await
            .expect("get secret")
            .is_none());
    }

    #[wasm_bindgen_test]
    async fn settings_store_transaction() {
        console_error_panic_hook::set_once();
        let ns = unique_namespace("settings");
        let store = WasmSettingsStore::new(ns).expect("settings store init");
        store.set_bool("first-run", true).await.expect("set bool");
        assert_eq!(
            store.get_bool("first-run").await.expect("get bool"),
            Some(true)
        );

        let mut txn = store.begin_transaction().await.expect("open txn");
        txn.set_string("theme", "dark").await.expect("stage value");
        txn.commit().await.expect("commit");

        assert_eq!(
            store.get_string("theme").await.expect("get string"),
            Some("dark".to_string())
        );

        let keys = store.list_keys().await.expect("list keys");
        assert!(keys.contains(&"first-run".to_string()));
        assert!(keys.contains(&"theme".to_string()));

        store.clear_all().await.expect("clear settings");
        assert!(store.list_keys().await.expect("list cleared").is_empty());
    }
}
