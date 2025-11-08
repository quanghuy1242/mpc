//! Encryption for offline cache files

use crate::error::{PlaybackError, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Encryption key for cache files.
///
/// Uses AES-256-GCM for authenticated encryption when the 'offline-cache' feature is enabled.
#[derive(Clone, Serialize, Deserialize)]
pub struct EncryptionKey {
    #[serde(with = "hex_serde")]
    key_bytes: Vec<u8>,
}

impl EncryptionKey {
    /// Generate a new random encryption key.
    #[cfg(feature = "offline-cache")]
    pub fn generate() -> Result<Self> {
        use aes_gcm::aead::OsRng;
        use aes_gcm::KeyInit;

        let key = aes_gcm::Aes256Gcm::generate_key(&mut OsRng);
        Ok(Self {
            key_bytes: key.to_vec(),
        })
    }

    /// Generate a new random encryption key (stub for when feature is disabled).
    #[cfg(not(feature = "offline-cache"))]
    pub fn generate() -> Result<Self> {
        Err(PlaybackError::EncryptionError(
            "Encryption not enabled. Enable 'offline-cache' feature.".to_string(),
        ))
    }

    /// Create from existing key bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(PlaybackError::EncryptionError(
                "Invalid key length. Expected 32 bytes for AES-256.".to_string(),
            ));
        }

        Ok(Self { key_bytes: bytes })
    }

    /// Get the key bytes (for secure storage).
    pub fn as_bytes(&self) -> &[u8] {
        &self.key_bytes
    }

    /// Serialize to hex string for storage.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.key_bytes)
    }

    /// Deserialize from hex string.
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str).map_err(|e| {
            PlaybackError::EncryptionError(format!("Invalid hex key: {}", e))
        })?;

        Self::from_bytes(bytes)
    }
}

impl fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptionKey")
            .field("key_bytes", &"[REDACTED]")
            .finish()
    }
}

/// Cache file encryptor using AES-256-GCM.
pub struct CacheEncryptor {
    key: EncryptionKey,
}

impl CacheEncryptor {
    /// Create a new encryptor with the given key.
    pub fn new(key: EncryptionKey) -> Self {
        Self { key }
    }

    /// Encrypt data for caching.
    #[cfg(feature = "offline-cache")]
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Bytes> {
        use aes_gcm::{
            aead::{Aead, KeyInit, OsRng},
            Aes256Gcm, Nonce,
        };

        // Create cipher instance
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&self.key.key_bytes);
        let cipher = Aes256Gcm::new(key);

        // Generate random nonce (12 bytes for GCM)
        let mut nonce_bytes = [0u8; 12];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|e| {
            PlaybackError::EncryptionError(format!("Encryption failed: {}", e))
        })?;

        // Prepend nonce to ciphertext for storage (first 12 bytes = nonce, rest = ciphertext)
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(Bytes::from(result))
    }

    /// Encrypt data (stub when feature is disabled).
    #[cfg(not(feature = "offline-cache"))]
    pub fn encrypt(&self, _plaintext: &[u8]) -> Result<Bytes> {
        Err(PlaybackError::EncryptionError(
            "Encryption not enabled. Enable 'offline-cache' feature.".to_string(),
        ))
    }

    /// Decrypt cached data.
    #[cfg(feature = "offline-cache")]
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Bytes> {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};

        // Check minimum length (nonce + at least some ciphertext + tag)
        if ciphertext.len() < 12 + 16 {
            return Err(PlaybackError::EncryptionError(
                "Invalid ciphertext: too short".to_string(),
            ));
        }

        // Extract nonce (first 12 bytes)
        let nonce = Nonce::from_slice(&ciphertext[..12]);

        // Extract actual ciphertext (after nonce)
        let encrypted_data = &ciphertext[12..];

        // Create cipher instance
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&self.key.key_bytes);
        let cipher = Aes256Gcm::new(key);

        // Decrypt
        let plaintext = cipher.decrypt(nonce, encrypted_data).map_err(|e| {
            PlaybackError::EncryptionError(format!("Decryption failed: {}", e))
        })?;

        Ok(Bytes::from(plaintext))
    }

    /// Decrypt cached data (stub when feature is disabled).
    #[cfg(not(feature = "offline-cache"))]
    pub fn decrypt(&self, _ciphertext: &[u8]) -> Result<Bytes> {
        Err(PlaybackError::EncryptionError(
            "Encryption not enabled. Enable 'offline-cache' feature.".to_string(),
        ))
    }
}

// Hex serialization helper for serde
mod hex_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_key_creation() {
        let key = EncryptionKey::from_bytes(vec![0u8; 32]).unwrap();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_encryption_key_invalid_length() {
        let result = EncryptionKey::from_bytes(vec![0u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn test_encryption_key_hex_roundtrip() {
        let key = EncryptionKey::from_bytes(vec![0x42u8; 32]).unwrap();
        let hex = key.to_hex();
        let restored = EncryptionKey::from_hex(&hex).unwrap();
        assert_eq!(key.as_bytes(), restored.as_bytes());
    }

    #[test]
    fn test_encryption_key_debug_redacted() {
        let key = EncryptionKey::from_bytes(vec![0xFFu8; 32]).unwrap();
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("FF"));
    }

    #[cfg(feature = "offline-cache")]
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = EncryptionKey::generate().unwrap();
        let encryptor = CacheEncryptor::new(key);

        let plaintext = b"Hello, World! This is a test message.";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();

        // Ciphertext should be different from plaintext
        assert_ne!(ciphertext.as_ref(), plaintext);

        // Decrypt should return original plaintext
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted.as_ref(), plaintext);
    }

    #[cfg(feature = "offline-cache")]
    #[test]
    fn test_encryption_produces_different_ciphertext() {
        let key = EncryptionKey::generate().unwrap();
        let encryptor = CacheEncryptor::new(key);

        let plaintext = b"Same message";
        let ciphertext1 = encryptor.encrypt(plaintext).unwrap();
        let ciphertext2 = encryptor.encrypt(plaintext).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(ciphertext1, ciphertext2);

        // Both should decrypt to the same plaintext
        let decrypted1 = encryptor.decrypt(&ciphertext1).unwrap();
        let decrypted2 = encryptor.decrypt(&ciphertext2).unwrap();
        assert_eq!(decrypted1, decrypted2);
        assert_eq!(decrypted1.as_ref(), plaintext);
    }

    #[cfg(feature = "offline-cache")]
    #[test]
    fn test_decrypt_invalid_ciphertext() {
        let key = EncryptionKey::generate().unwrap();
        let encryptor = CacheEncryptor::new(key);

        // Too short
        let result = encryptor.decrypt(&[0u8; 10]);
        assert!(result.is_err());

        // Tampered ciphertext
        let plaintext = b"Test message";
        let mut ciphertext = encryptor.encrypt(plaintext).unwrap().to_vec();
        ciphertext[15] ^= 0xFF; // Flip a bit

        let result = encryptor.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[cfg(not(feature = "offline-cache"))]
    #[test]
    fn test_encryption_disabled_returns_error() {
        let result = EncryptionKey::generate();
        assert!(result.is_err());

        let key = EncryptionKey::from_bytes(vec![0u8; 32]).unwrap();
        let encryptor = CacheEncryptor::new(key);

        let encrypt_result = encryptor.encrypt(b"test");
        assert!(encrypt_result.is_err());

        let decrypt_result = encryptor.decrypt(b"test");
        assert!(decrypt_result.is_err());
    }
}
