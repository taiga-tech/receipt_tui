//! Token storage implementation used by OAuth.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::ErrorKind, path::PathBuf};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};
use yup_oauth2::storage::{TokenInfo, TokenStorage, TokenStorageError};

/// Stores OAuth tokens in a local JSON file (token.json).
#[derive(Clone)]
pub struct FileTokenStorage {
    /// Location of the token cache on disk.
    path: PathBuf,
}

impl FileTokenStorage {
    /// Create a new storage backed by the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Stable hash of the scope list (order-insensitive).
    fn scopes_key(scopes: &[&str]) -> String {
        let mut v: Vec<&str> = scopes.iter().copied().collect();
        v.sort_unstable();
        v.dedup();
        let joined = v.join(" ");
        let hash = Sha256::digest(joined.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    }

    /// Key used in the token map for the given scopes.
    fn entry_key(scopes: &[&str]) -> String {
        format!("oauth_token:{}", Self::scopes_key(scopes))
    }

    /// Load the entire token map from disk.
    async fn load_map(&self) -> Result<HashMap<String, TokenInfo>, TokenStorageError> {
        match fs::read(&self.path).await {
            Ok(data) => {
                if data.is_empty() {
                    return Ok(HashMap::new());
                }
                serde_json::from_slice(&data)
                    .map_err(|e| TokenStorageError::Other(e.to_string().into()))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(TokenStorageError::Other(e.to_string().into())),
        }
    }

    /// Persist the token map to disk, creating directories if needed.
    async fn save_map(&self, map: &HashMap<String, TokenInfo>) -> Result<(), TokenStorageError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
            }
        }
        let data = serde_json::to_vec_pretty(map)
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        let file = fs::File::create(&self.path)
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(&data)
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        writer
            .flush()
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        Ok(())
    }
}

#[async_trait]
impl TokenStorage for FileTokenStorage {
    /// Store or replace the token for the given scopes.
    async fn set(&self, scopes: &[&str], token: TokenInfo) -> Result<(), TokenStorageError> {
        let mut map = self.load_map().await?;
        let key = Self::entry_key(scopes);
        map.insert(key, token);
        self.save_map(&map).await
    }

    /// Retrieve the token for the given scopes, if present.
    async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
        let mut map = self.load_map().await.ok()?;
        let key = Self::entry_key(scopes);
        map.remove(&key)
    }
}
