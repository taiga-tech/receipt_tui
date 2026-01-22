//! Google Sheets APIのヘルパー。

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// スプレッドシートレスポンスの最小ラッパー。
#[derive(Debug, Deserialize)]
pub struct Spreadsheet {
    pub sheets: Vec<Sheet>,
}
/// スプレッドシート内のシート情報。
#[derive(Debug, Deserialize)]
pub struct Sheet {
    pub properties: SheetProps,
}
/// アプリが利用するシートのプロパティ。
#[derive(Debug, Deserialize)]
pub struct SheetProps {
    pub title: String,
    #[serde(default)]
    pub grid_properties: Option<GridProps>,
}
/// グリッド情報（行数など）。
#[derive(Debug, Deserialize)]
pub struct GridProps {
    pub row_count: Option<u32>,
}

/// 最初のシート名と行数を取得する（行数は推定含む）。
pub async fn get_first_sheet_title_and_rows(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
) -> Result<(String, u32)> {
    // シート情報だけを取得するURLを組み立てる。
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}?fields=sheets(properties(title,gridProperties(rowCount)))",
        spreadsheet_id
    );
    // HTTPリクエストを実行し、成功レスポンスへ正規化する。
    let resp = http.get(url).bearer_auth(token).send().await?;
    let resp = ensure_success(resp).await?;
    // JSONを構造体へデコードする。
    let ss = resp.json::<Spreadsheet>().await?;

    // 最初のシートを取り出す。
    let s0 = ss.sheets.first().ok_or_else(|| anyhow!("no sheets"))?;
    let title = s0.properties.title.clone();
    let rows = s0
        .properties
        .grid_properties
        .as_ref()
        .and_then(|g| g.row_count)
        // グリッド情報が無い場合は妥当なサイズで代用する。
        .unwrap_or(1000);
    Ok((title, rows))
}

/// 既存行数カウントに使うValuesレスポンス。
#[derive(Debug, Deserialize)]
struct ValuesGetResp {
    #[serde(default)]
    values: Vec<Vec<String>>,
}

/// 指定列で連続する非空行数をカウントする。
pub async fn count_existing_rows_in_col(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
    sheet_title: &str,
    col: &str,
    start_row: u32,
) -> Result<u32> {
    // 読み取り範囲をA1形式で組み立てる。
    let range = format!("{}!{}{}:{}", sheet_title, col, start_row, col);
    // Values取得用URLを構築する。
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        spreadsheet_id,
        urlencoding::encode(&range)
    );
    // HTTPリクエストを実行し、成功レスポンスへ正規化する。
    let resp = http.get(url).bearer_auth(token).send().await?;
    let resp = ensure_success(resp).await?;
    // JSONを構造体へデコードする。
    let resp = resp.json::<ValuesGetResp>().await?;

    // 先頭から空セルに当たるまで数える。
    let mut n = 0u32;
    for row in resp.values {
        let v = row.first().map(|s| s.trim()).unwrap_or("");
        // 空セルに到達したら、次の挿入位置なので停止する。
        if v.is_empty() {
            break;
        }
        n += 1;
    }
    Ok(n)
}

/// バッチ更新APIのリクエストボディ。
#[derive(Debug, Serialize)]
struct BatchUpdateReq<'a> {
    value_input_option: &'a str,
    data: Vec<ValueRange<'a>>,
}

/// バッチ更新内の1レンジ更新。
#[derive(Debug, Serialize)]
struct ValueRange<'a> {
    range: String,
    values: Vec<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    major_dimension: Option<&'a str>,
}

/// 複数レンジの更新を1回のAPIで適用する。
pub async fn values_batch_update(
    http: &Client,
    token: &str,
    spreadsheet_id: &str,
    updates: Vec<(String, Vec<Vec<serde_json::Value>>)>,
) -> Result<()> {
    // 更新データをValueRangeへ変換する。
    let data = updates
        .into_iter()
        .map(|(range, values)| ValueRange {
            range,
            values,
            major_dimension: None,
        })
        .collect();

    // リクエストボディを組み立てる。
    let body = BatchUpdateReq {
        value_input_option: "USER_ENTERED",
        data,
    };

    // バッチ更新APIのURLを作成する。
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values:batchUpdate",
        spreadsheet_id
    );

    // HTTPリクエストを実行して成功を確認する。
    let resp = http.post(url).bearer_auth(token).json(&body).send().await?;
    ensure_success(resp).await?;
    Ok(())
}

/// 非2xxレスポンスを構造化エラーに変換する。
async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response> {
    // ステータスコードを取得する。
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    // ボディ内容を文字列化してエラーメッセージへ含める。
    let body = resp.text().await.unwrap_or_else(|_| "".into());
    Err(anyhow!("HTTP status {status} error: {body}"))
}
