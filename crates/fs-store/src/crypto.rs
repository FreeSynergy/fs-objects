//! Encryption-at-rest hook (ADR 0024).
//!
//! Pattern: Strategy — [`KeyProvider`] is the strategy for obtaining a key (static, vault, ...).
//!
//! The store never manages keys itself. A [`KeyProvider`] hands it a key when
//! the store is opened; the future key vault implements this trait.
//! The `TursoStore` maps the key onto Turso's experimental page-level
//! encryption (the whole file and WAL are encrypted).

use crate::{Error, Result};

/// Ciphers supported by Turso's page encryption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cipher {
    /// AES-128-GCM (16-byte key).
    Aes128Gcm,
    /// AES-256-GCM (32-byte key).
    Aes256Gcm,
    /// AEGIS-256 (32-byte key).
    Aegis256,
    /// AEGIS-128L (16-byte key).
    Aegis128L,
}

impl Cipher {
    /// Required key length in bytes.
    pub fn key_len(&self) -> usize {
        match self {
            Cipher::Aes128Gcm | Cipher::Aegis128L => 16,
            Cipher::Aes256Gcm | Cipher::Aegis256 => 32,
        }
    }
    pub(crate) fn turso_name(self) -> &'static str {
        match self {
            Cipher::Aes128Gcm => "aes128gcm",
            Cipher::Aes256Gcm => "aes256gcm",
            Cipher::Aegis256 => "aegis256",
            Cipher::Aegis128L => "aegis128l",
        }
    }
}

/// A raw key for one store.
#[derive(Clone)]
pub struct EncryptionKey {
    /// The cipher the key is for.
    pub cipher: Cipher,
    /// Raw key bytes; length must match [`Cipher::key_len`].
    pub key: Vec<u8>,
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionKey")
            .field("cipher", &self.cipher)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl EncryptionKey {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.key.len() != self.cipher.key_len() {
            return Err(Error::Key(format!(
                "{:?} needs a {}-byte key, got {}",
                self.cipher,
                self.cipher.key_len(),
                self.key.len()
            )));
        }
        Ok(())
    }
    pub(crate) fn hex(&self) -> String {
        use std::fmt::Write as _;
        self.key.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

/// Source of the store key (ADR 0024: key vault, later).
pub trait KeyProvider: Send + Sync {
    /// Key for the store identified by `store_name` (typically the program name / file stem).
    fn key_for(&self, store_name: &str) -> Result<EncryptionKey>;
}

/// A fixed key; for tests and for callers that obtained the key elsewhere.
pub struct StaticKeyProvider(pub EncryptionKey);

impl KeyProvider for StaticKeyProvider {
    fn key_for(&self, _store_name: &str) -> Result<EncryptionKey> {
        Ok(self.0.clone())
    }
}
