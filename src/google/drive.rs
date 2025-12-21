//! Google Drive API helpers.

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// List response for Drive files.
#[derive(Debug, Deserialize)]
pub struct FileListResp {
    pub files: Vec<DriveFile>,
}

/// Minimal Drive file metadata needed by the app.
#[derive(Debug, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
}

/// Metadata used to resolve shortcuts into real files.
#[derive(Debug, Deserialize)]
struct FileMeta {
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "shortcutDetails")]
    shortcut_details: Option<ShortcutDetails>,
}

/// Shortcut details returned by Drive API.
#[derive(Debug, Deserialize)]
struct ShortcutDetails {
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "targetMimeType")]
    target_mime_type: String,
}

/// List image files in a Drive folder.
pub async fn list_images_in_folder(
    http: &Client,
    token: &str,
    folder_id: &str,
) -> Result<Vec<DriveFile>> {
    // Query for non-trashed images in the given parent.
    let q = format!(
        "'{}' in parents and trashed=false and mimeType contains 'image/'",
        folder_id
    );
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)",
        urlencoding::encode(&q)
    );

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

/// Resolve a template id that may be a shortcut into a real sheet id.
pub async fn resolve_sheet_id(http: &Client, token: &str, file_id: &str) -> Result<String> {
    const SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
    const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";

    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?fields=mimeType,shortcutDetails(targetId,targetMimeType)",
        file_id
    );
    let meta = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<FileMeta>()
        .await?;

    match meta.mime_type.as_str() {
        SHEET_MIME => Ok(file_id.to_string()),
        SHORTCUT_MIME => {
            let details = meta
                .shortcut_details
                .ok_or_else(|| anyhow!("shortcutDetails missing for template_sheet_id"))?;
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

/// Drive copy request body.
#[derive(Debug, Serialize)]
struct CopyReq<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parents: Option<Vec<&'a str>>,
}

/// Copy a Drive file and return the new file id.
pub async fn copy_file(
    http: &Client,
    token: &str,
    file_id: &str,
    new_name: &str,
    parent_folder_id: Option<&str>,
) -> Result<String> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/copy?fields=id",
        file_id
    );
    let body = CopyReq {
        name: new_name,
        parents: parent_folder_id.map(|p| vec![p]),
    };
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

/// Export a spreadsheet to PDF.
pub async fn export_pdf(http: &Client, token: &str, sheet_file_id: &str) -> Result<Vec<u8>> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/export?mimeType=application/pdf",
        sheet_file_id
    );

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

/// Upload a PDF into a Drive folder and return its file id.
pub async fn upload_pdf(
    http: &Client,
    token: &str,
    parent_folder_id: &str,
    filename: &str,
    pdf_bytes: Vec<u8>,
) -> Result<String> {
    let meta = serde_json::json!({
        "name": filename,
        "parents": [parent_folder_id],
        "mimeType": "application/pdf"
    });

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
