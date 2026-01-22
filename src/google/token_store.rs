//! OAuthで使うトークン保存実装。

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::ErrorKind, path::PathBuf};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};
use yup_oauth2::storage::{TokenInfo, TokenStorage, TokenStorageError};

/// OAuthトークンをローカルJSON（token.json）に保存する。
#[derive(Clone)]
pub struct FileTokenStorage {
    /// トークンキャッシュの保存先。
    path: PathBuf,
}

impl FileTokenStorage {
    /// 指定パスで新しいストレージを作成する。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// スコープ配列の順序に依存しない安定ハッシュ。
    fn scopes_key(scopes: &[&str]) -> String {
        // スコープをソートして重複を除去する。
        let mut v: Vec<&str> = scopes.to_vec();
        v.sort_unstable();
        v.dedup();
        // 連結文字列をハッシュ化する。
        let joined = v.join(" ");
        let hash = Sha256::digest(joined.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    }

    /// スコープごとのトークンマップキー。
    fn entry_key(scopes: &[&str]) -> String {
        format!("oauth_token:{}", Self::scopes_key(scopes))
    }

    /// ディスクからトークンマップ全体を読み込む。
    async fn load_map(&self) -> Result<HashMap<String, TokenInfo>, TokenStorageError> {
        match fs::read(&self.path).await {
            Ok(data) => {
                // 空ファイルの場合は空マップとして扱う。
                if data.is_empty() {
                    return Ok(HashMap::new());
                }
                // JSONをマップへデシリアライズする。
                serde_json::from_slice(&data)
                    .map_err(|e| TokenStorageError::Other(e.to_string().into()))
            }
            // ファイル未作成は空マップとして扱う。
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(TokenStorageError::Other(e.to_string().into())),
        }
    }

    /// トークンマップをディスクへ保存（必要ならディレクトリ作成）。
    async fn save_map(&self, map: &HashMap<String, TokenInfo>) -> Result<(), TokenStorageError> {
        // 親ディレクトリが指定されていれば作成する。
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        }
        // JSONを整形してバイト列へ変換する。
        let data = serde_json::to_vec_pretty(map)
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        // ファイルを作成して書き込む。
        let file = fs::File::create(&self.path)
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(&data)
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        // フラッシュして確実に保存する。
        writer
            .flush()
            .await
            .map_err(|e| TokenStorageError::Other(e.to_string().into()))?;
        Ok(())
    }
}

#[async_trait]
impl TokenStorage for FileTokenStorage {
    /// 指定スコープのトークンを保存/更新する。
    async fn set(&self, scopes: &[&str], token: TokenInfo) -> Result<(), TokenStorageError> {
        // 既存マップを読み込み、キーで置き換える。
        let mut map = self.load_map().await?;
        let key = Self::entry_key(scopes);
        map.insert(key, token);
        // 更新後のマップを保存する。
        self.save_map(&map).await
    }

    /// 指定スコープのトークンを取得する。
    async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
        // マップを読み込み、キーで取り出す。
        let mut map = self.load_map().await.ok()?;
        let key = Self::entry_key(scopes);
        map.remove(&key)
    }
}
