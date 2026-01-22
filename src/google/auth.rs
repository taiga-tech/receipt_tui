//! Google API向けOAuth設定とスコープ管理。

use anyhow::Result;
use std::{future::Future, pin::Pin, result::Result as StdResult};
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::authenticator_delegate::{DefaultInstalledFlowDelegate, InstalledFlowDelegate};
use yup_oauth2::{
    DefaultHyperClientBuilder, HyperClientBuilder, InstalledFlowAuthenticator,
    InstalledFlowReturnMethod,
};

use super::token_store::FileTokenStorage;

/// アプリ全体で使うAuthenticator型。
pub type InstalledAuth =
    Authenticator<<DefaultHyperClientBuilder as HyperClientBuilder>::Connector>;

#[derive(Copy, Clone)]
/// ブラウザ起動後、標準のフロー処理へ委譲するデリゲート。
struct InstalledFlowBrowserDelegate;

/// ブラウザを起動し、標準のインストールフローへフォールバックする。
async fn browser_user_url(url: &str, need_code: bool) -> StdResult<String, String> {
    // 認証URLをブラウザで開く（失敗は無視）。
    let _ = webbrowser::open(url);
    // 既定のフローでユーザー入力を促す。
    let def_delegate = DefaultInstalledFlowDelegate;
    def_delegate.present_user_url(url, need_code).await
}

impl InstalledFlowDelegate for InstalledFlowBrowserDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = StdResult<String, String>> + Send + 'a>> {
        // 非同期でブラウザ起動→コード取得を行う。
        Box::pin(browser_user_url(url, need_code))
    }
}

/// ファイル保存型トークンストレージでAuthenticatorを構築する。
pub async fn authenticator() -> Result<InstalledAuth> {
    // OAuthクライアントシークレットを埋め込みで読み込む。
    const CREDS: &str = include_str!("../../assets/credentials.json");
    // クライアント情報をパースする。
    let secret = yup_oauth2::parse_application_secret(CREDS.as_bytes())?;

    // トークン保存先を準備する。
    let storage = FileTokenStorage::new("token.json");

    // Installed Flow用のAuthenticatorを構築する。
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .with_storage(Box::new(storage))
        .flow_delegate(Box::new(InstalledFlowBrowserDelegate))
        .build()
        .await?;

    Ok(auth)
}

/// Drive/Sheets操作に必要なOAuthスコープ。
pub fn scopes() -> Vec<&'static str> {
    vec![
        "https://www.googleapis.com/auth/drive",
        "https://www.googleapis.com/auth/spreadsheets",
    ]
}
