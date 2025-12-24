//! Google Sheets API helpers.

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Minimal spreadsheet response wrapper.
#[derive(Debug, Deserialize)]
pub struct Spreadsheet {
    pub sheets: Vec<Sheet>,
}
/// Sheet container within the spreadsheet.
#[derive(Debug, Deserialize)]
pub struct Sheet {
    pub properties: SheetProps,
}
/// Sheet properties used by the app.
#[derive(Debug, Deserialize)]
pub struct SheetProps {
    pub title: String,
    #[serde(default)]
    pub grid_properties: Option<GridProps>,
}
/// Grid properties (row count) if available.
#[derive(Debug, Deserialize)]
pub struct GridProps {
    pub row_count: Option<u32>,
}

/// Fetch the first sheet title and its row count (best effort).
pub async fn get_first_sheet_title_and_rows(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
) -> Result<(String, u32)> {
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}?fields=sheets(properties(title,gridProperties(rowCount)))",
        spreadsheet_id
    );
    let resp = http.get(url).bearer_auth(token).send().await?;
    let resp = ensure_success(resp).await?;
    let ss = resp.json::<Spreadsheet>().await?;

    let s0 = ss.sheets.first().ok_or_else(|| anyhow!("no sheets"))?;
    let title = s0.properties.title.clone();
    let rows = s0
        .properties
        .grid_properties
        .as_ref()
        .and_then(|g| g.row_count)
        // Default to a reasonable size when grid properties are absent.
        .unwrap_or(1000);
    Ok((title, rows))
}

/// Values response used to count existing rows.
#[derive(Debug, Deserialize)]
struct ValuesGetResp {
    #[serde(default)]
    values: Vec<Vec<String>>,
}

/// Count contiguous non-empty rows from a start row in a column.
pub async fn count_existing_rows_in_col(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
    sheet_title: &str,
    col: &str,
    start_row: u32,
) -> Result<u32> {
    let range = format!("{}!{}{}:{}", sheet_title, col, start_row, col);
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        spreadsheet_id,
        urlencoding::encode(&range)
    );
    let resp = http.get(url).bearer_auth(token).send().await?;
    let resp = ensure_success(resp).await?;
    let resp = resp.json::<ValuesGetResp>().await?;

    let mut n = 0u32;
    for row in resp.values {
        let v = row.first().map(|s| s.trim()).unwrap_or("");
        // Stop at the first empty cell to find the next insertion point.
        if v.is_empty() {
            break;
        }
        n += 1;
    }
    Ok(n)
}

/// Batch update request body.
#[derive(Debug, Serialize)]
struct BatchUpdateReq<'a> {
    value_input_option: &'a str,
    data: Vec<ValueRange<'a>>,
}

/// A single value range update in the batch request.
#[derive(Debug, Serialize)]
struct ValueRange<'a> {
    range: String,
    values: Vec<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    major_dimension: Option<&'a str>,
}

/// Apply multiple value updates in one API call.
pub async fn values_batch_update(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
    updates: Vec<(String, Vec<Vec<serde_json::Value>>)>,
) -> Result<()> {
    let data = updates
        .into_iter()
        .map(|(range, values)| ValueRange {
            range,
            values,
            major_dimension: None,
        })
        .collect();

    let body = BatchUpdateReq {
        value_input_option: "USER_ENTERED",
        data,
    };

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values:batchUpdate",
        spreadsheet_id
    );

    let resp = http.post(url).bearer_auth(token).json(&body).send().await?;
    ensure_success(resp).await?;
    Ok(())
}

/// Convert non-2xx responses into a structured error.
async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_else(|_| "".into());
    Err(anyhow!("HTTP status {status} error: {body}"))
}
