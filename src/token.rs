//! Bearer-token store.
//!
//! Tokens are random 32-byte values encoded as URL-safe base64. The user sees
//! the token exactly once at grant time; thereafter only its SHA-256 hash is
//! stored on disk. Tokens live in `~/.config/ron/tokens.json`, not in the
//! git-tracked dataset.
//!
//! Token management endpoints (`grant`/`revoke`/`list`) don't require auth;
//! they're reachable only because the server binds to localhost. All other
//! endpoints require a valid bearer token.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{Local, NaiveDateTime};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A persisted token record. `hash` is the SHA-256 hex of the raw secret.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRecord {
    pub id: String,
    pub label: String,
    pub hash: String,
    pub created: NaiveDateTime,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct TokenStore {
    pub tokens: Vec<TokenRecord>,
}

impl TokenStore {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Mint a new token. Returns the raw secret (only visible at this moment)
    /// and the record that was stored.
    pub fn grant(&mut self, label: impl Into<String>) -> (String, TokenRecord) {
        let mut buf = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf);
        let hash = hex(&Sha256::digest(secret.as_bytes()));
        let record = TokenRecord {
            id: rand_id(),
            label: label.into(),
            hash,
            created: Local::now().naive_local(),
        };
        self.tokens.push(record.clone());
        (secret, record)
    }

    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.tokens.len();
        self.tokens.retain(|t| t.id != id);
        self.tokens.len() != before
    }

    pub fn list(&self) -> &[TokenRecord] {
        &self.tokens
    }

    /// Validate a presented secret. Returns true if it matches some stored hash.
    pub fn validate(&self, secret: &str) -> bool {
        let hash = hex(&Sha256::digest(secret.as_bytes()));
        self.tokens.iter().any(|t| t.hash == hash)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn rand_id() -> String {
    let mut buf = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut buf);
    hex(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn grant_then_validate() {
        let mut store = TokenStore::default();
        let (secret, record) = store.grant("browser");
        assert!(!secret.is_empty());
        assert_eq!(record.label, "browser");
        assert!(store.validate(&secret));
        assert!(!store.validate("bogus"));
    }

    #[test]
    fn revoke_drops_token() {
        let mut store = TokenStore::default();
        let (_, r1) = store.grant("a");
        let (s2, r2) = store.grant("b");
        assert!(store.revoke(&r1.id));
        assert!(!store.validate(&"not-real".to_string()));
        assert!(store.validate(&s2));
        assert_eq!(store.tokens.len(), 1);
        assert_eq!(store.tokens[0].id, r2.id);
    }

    #[test]
    fn store_round_trips_through_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tokens.json");
        let mut store = TokenStore::default();
        let (secret, _) = store.grant("browser");
        store.save(&path).unwrap();
        let back = TokenStore::load(&path).unwrap();
        assert_eq!(back.tokens.len(), 1);
        assert!(back.validate(&secret));
        // The raw secret is not stored.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains(&secret));
    }

    #[test]
    fn load_handles_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let store = TokenStore::load(&path).unwrap();
        assert!(store.tokens.is_empty());
    }
}
