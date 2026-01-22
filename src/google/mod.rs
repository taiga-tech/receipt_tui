//! Google APIクライアントのヘルパー群。

/// OAuthとスコープ周りのヘルパー。
pub mod auth;
/// Drive APIのラッパー。
pub mod drive;
/// Sheets APIのラッパー。
pub mod sheets;
/// OAuthトークンの保存処理。
pub mod token_store;
