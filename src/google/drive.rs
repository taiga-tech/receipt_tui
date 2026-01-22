//! Google Drive APIのヘルパー。

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Driveファイル一覧のレスポンス。
#[derive(Debug, Deserialize)]
pub struct FileListResp {
    pub files: Vec<DriveFile>,
}

/// アプリが必要とする最小限のDriveファイル情報。
#[derive(Debug, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
}

/// ショートカット解決に使うメタデータ。
#[derive(Debug, Deserialize)]
struct FileMeta {
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "shortcutDetails")]
    shortcut_details: Option<ShortcutDetails>,
}

/// Drive APIから返るショートカット詳細。
#[derive(Debug, Deserialize)]
struct ShortcutDetails {
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "targetMimeType")]
    target_mime_type: String,
}

/// 指定フォルダ内の画像ファイルを一覧取得する。
pub async fn list_images_in_folder(
    http: &Client,
    token: &str,
    folder_id: &str,
) -> Result<Vec<DriveFile>> {
    // 対象フォルダ配下の画像（ゴミ箱除外）を検索する。
    let q = format!(
        "'{}' in parents and trashed=false and mimeType contains 'image/'",
        folder_id
    );
    // Drive APIのクエリURLを組み立てる。
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)",
        urlencoding::encode(&q)
    );

    // HTTPリクエストを送信し、レスポンスを解析する。
    let resp = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<FileListResp>()
        .await?;

    Ok(resp.files)
}

/// テンプレートIDがショートカットの場合、実体のシートIDへ解決する。
pub async fn resolve_sheet_id(http: &Client, token: &str, file_id: &str) -> Result<String> {
    const SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
    const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";

    // メタデータ取得用のURLを組み立てる。
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?fields=mimeType,shortcutDetails(targetId,targetMimeType)",
        file_id
    );
    // メタデータを取得してJSONへパースする。
    let meta = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<FileMeta>()
        .await?;

    // MIMEタイプに応じてIDを返す。
    match meta.mime_type.as_str() {
        SHEET_MIME => Ok(file_id.to_string()),
        SHORTCUT_MIME => {
            // ショートカットのターゲット情報を取り出す。
            let details = meta
                .shortcut_details
                .ok_or_else(|| anyhow!("shortcutDetails missing for template_sheet_id"))?;
            // ターゲットがシートならそのIDを返す。
            if details.target_mime_type == SHEET_MIME {
                Ok(details.target_id)
            } else {
                Err(anyhow!(
                    "template_sheet_id must point to a Google Sheets file (shortcut target is {})",
                    details.target_mime_type
                ))
            }
        }
        other => Err(anyhow!(
            "template_sheet_id must point to a Google Sheets file (got {})",
            other
        )),
    }
}

/// DriveコピーAPIのリクエストボディ。
#[derive(Debug, Serialize)]
struct CopyReq<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parents: Option<Vec<&'a str>>,
}

/// Driveファイルをコピーし、新しいファイルIDを返す。
pub async fn copy_file(
    http: &Client,
    token: &str,
    file_id: &str,
    new_name: &str,
    parent_folder_id: Option<&str>,
) -> Result<String> {
    // コピーAPIのURLを組み立てる。
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/copy?fields=id",
        file_id
    );
    // リクエストボディを作成する。
    let body = CopyReq {
        name: new_name,
        parents: parent_folder_id.map(|p| vec![p]),
    };
    // HTTPリクエストを実行してIDを取得する。
    let v = http
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(v["id"]
        .as_str()
        .ok_or_else(|| anyhow!("no id"))?
        .to_string())
}

/// スプレッドシートをPDFとしてエクスポートする。
pub async fn export_pdf(http: &Client, token: &str, sheet_file_id: &str) -> Result<Vec<u8>> {
    // エクスポート用URLを作る。
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/export?mimeType=application/pdf",
        sheet_file_id
    );

    // PDFのバイナリを取得する。
    let bytes = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

/// PDFをDriveへアップロードし、ファイルIDを返す。
pub async fn upload_pdf(
    http: &Client,
    token: &str,
    parent_folder_id: &str,
    filename: &str,
    pdf_bytes: Vec<u8>,
) -> Result<String> {
    // メタデータ（ファイル名・親フォルダ・MIME）を用意する。
    let meta = serde_json::json!({
        "name": filename,
        "parents": [parent_folder_id],
        "mimeType": "application/pdf"
    });

    // マルチパートフォーム（メタデータ＋ファイル本体）を構築する。
    let form = reqwest::multipart::Form::new()
        .part(
            "metadata",
            reqwest::multipart::Part::text(meta.to_string())
                .mime_str("application/json; charset=UTF-8")?,
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(pdf_bytes)
                .file_name(filename.to_string())
                .mime_str("application/pdf")?,
        );

    // アップロードAPIを実行してIDを取得する。
    let url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id";
    let v = http
        .post(url)
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(v["id"]
        .as_str()
        .ok_or_else(|| anyhow!("no id"))?
        .to_string())
}
