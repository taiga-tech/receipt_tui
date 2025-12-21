//! OAuth setup and scopes for Google APIs.

use anyhow::Result;
use std::{future::Future, pin::Pin, result::Result as StdResult};
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::authenticator_delegate::{DefaultInstalledFlowDelegate, InstalledFlowDelegate};
use yup_oauth2::{
    DefaultHyperClientBuilder, HyperClientBuilder, InstalledFlowAuthenticator,
    InstalledFlowReturnMethod,
};

use super::token_store::FileTokenStorage;

/// Authenticator type used across the app.
pub type InstalledAuth =
    Authenticator<<DefaultHyperClientBuilder as HyperClientBuilder>::Connector>;

#[derive(Copy, Clone)]
/// Opens the browser and delegates to the default flow handler.
struct InstalledFlowBrowserDelegate;

/// Launch the browser and fall back to the default installed flow prompt.
async fn browser_user_url(url: &str, need_code: bool) -> StdResult<String, String> {
    let _ = webbrowser::open(url);
    let def_delegate = DefaultInstalledFlowDelegate;
    def_delegate.present_user_url(url, need_code).await
}

impl InstalledFlowDelegate for InstalledFlowBrowserDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = StdResult<String, String>> + Send + 'a>> {
        Box::pin(browser_user_url(url, need_code))
    }
}

/// Build an installed-flow authenticator with file-backed token storage.
pub async fn authenticator() -> Result<InstalledAuth> {
    // OAuth client secret is embedded for the local app.
    const CREDS: &str = include_str!("../../assets/credentials.json");
    let secret = yup_oauth2::parse_application_secret(CREDS.as_bytes())?;

    let storage = FileTokenStorage::new("token.json");

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .with_storage(Box::new(storage))
        .flow_delegate(Box::new(InstalledFlowBrowserDelegate))
        .build()
        .await?;

    Ok(auth)
}

/// OAuth scopes required for Drive and Sheets operations.
pub fn scopes() -> Vec<&'static str> {
    vec![
        "https://www.googleapis.com/auth/drive",
        "https://www.googleapis.com/auth/spreadsheets",
    ]
}
