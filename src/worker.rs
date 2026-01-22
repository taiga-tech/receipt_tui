//! Google APIジョブを処理するバックグラウンドワーカー。

use crate::{
    config::Config,
    google::{auth, drive, sheets},
    jobs::{Job, JobStatus, ReceiptFields},
};
use anyhow::{Result, anyhow};
use reqwest::Client;
use tokio::sync::mpsc;

/// UIからWorkerへ送るコマンド。
#[derive(Debug)]
pub enum WorkerCmd {
    /// Driveを再スキャンして入力画像を取得する。
    RefreshJobs,
    /// 設定を保存し反映する。
    SaveSettings(Config),
    /// 編集内容を書き込み、PDFをエクスポート/アップロードする。
    CommitJobEdits {
        job_id: uuid::Uuid,
        fields: ReceiptFields,
        target_month_ym: String,
    },
}

/// UI更新用にWorkerから送るイベント。
#[derive(Clone, Debug)]
pub enum WorkerEvent {
    /// Driveから取得したジョブ一覧。
    JobsLoaded(Vec<Job>),
    /// 単一ジョブのステータス更新。
    JobUpdated {
        job_id: uuid::Uuid,
        status: JobStatus,
    },
    /// 情報ログ。
    Log(String),
    /// ユーザーに見せるエラーメッセージ。
    Error(String),
}

/// ワーカーメインループ：認証後、コマンドを逐次処理する。
pub async fn run(
    mut rx: mpsc::Receiver<WorkerCmd>,
    tx: mpsc::Sender<WorkerEvent>,
    mut cfg: Config,
) {
    // 全API呼び出しで共有するHTTPクライアント。
    let http = Client::new();
    tracing::info!("worker started");

    // OAuth初期化は一度だけ行い、失敗時は終了する。
    let authn = match auth::authenticator().await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("OAuth init failed: {e}");
            // UIへエラーを通知して終了する。
            let _ = tx
                .send(WorkerEvent::Error(format!("OAuth init failed: {e}")))
                .await;
            return;
        }
    };
    tracing::info!("OAuth authenticator ready");

    // 状態整合性のため、コマンドは逐次処理する。
    while let Some(cmd) = rx.recv().await {
        match cmd {
            WorkerCmd::SaveSettings(new_cfg) => {
                tracing::info!("settings updated");
                // 設定を更新してログ通知する。
                cfg = new_cfg;
                let _ = tx.send(WorkerEvent::Log("settings updated".into())).await;
            }

            WorkerCmd::RefreshJobs => {
                tracing::info!("refresh jobs");
                // Driveアクセス前に最低限の設定があるか確認する。
                if cfg.google.input_folder_id.is_empty() {
                    tracing::warn!("refresh aborted: input_folder_id missing");
                    let _ = tx
                        .send(WorkerEvent::Error("input_folder_id is not set".into()))
                        .await;
                    continue;
                }

                match access_token(&authn).await {
                    Ok(token) => {
                        tracing::info!("access token acquired");
                        // 画像ファイル一覧を取得し、編集可能なジョブへ変換する。
                        match drive::list_images_in_folder(
                            &http,
                            &token,
                            &cfg.google.input_folder_id,
                        )
                        .await
                        {
                            Ok(files) => {
                                tracing::info!("drive list success: {} files", files.len());
                                // 各ファイルをジョブに変換し、初期状態をセットする。
                                let jobs = files
                                    .into_iter()
                                    .map(|f| {
                                        let mut j = Job::new(f.id, f.name);
                                        // ユーザーが編集できるよう初期状態を設定する。
                                        j.status = JobStatus::WaitingUserFix;
                                        j
                                    })
                                    .collect::<Vec<_>>();
                                // UIへ一覧更新イベントを送る。
                                let _ = tx.send(WorkerEvent::JobsLoaded(jobs)).await;
                            }
                            Err(e) => {
                                tracing::error!("drive list failed: {e}");
                                // 取得失敗をUIへ通知する。
                                let _ = tx
                                    .send(WorkerEvent::Error(format!("list failed: {e}")))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("token failed: {e}");
                        // トークン取得失敗をUIへ通知する。
                        let _ = tx
                            .send(WorkerEvent::Error(format!("token failed: {e}")))
                            .await;
                    }
                }
            }

            WorkerCmd::CommitJobEdits {
                job_id,
                fields,
                target_month_ym,
            } => {
                tracing::info!("commit job start: {job_id}");
                // UIに即時反映させるためステータスを先に更新する。
                let _ = tx
                    .send(WorkerEvent::JobUpdated {
                        job_id,
                        status: JobStatus::WritingSheet,
                    })
                    .await;

                // 実際の書き込み/エクスポート/アップロードを行う。
                let r =
                    commit_one(&http, &authn, &cfg, &fields, &target_month_ym, &tx, job_id).await;
                match r {
                    Ok(_) => {
                        tracing::info!("commit job done: {job_id}");
                        // 完了状態へ更新する。
                        let _ = tx
                            .send(WorkerEvent::JobUpdated {
                                job_id,
                                status: JobStatus::Done,
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::error!("commit job failed: {job_id}: {e}");
                        // 失敗状態へ更新し、エラー内容を伝える。
                        let _ = tx
                            .send(WorkerEvent::JobUpdated {
                                job_id,
                                status: JobStatus::Error(e.to_string()),
                            })
                            .await;
                    }
                }
            }
        }
    }
}

/// Authenticatorから新しいアクセストークンを取得する。
async fn access_token(authn: &auth::InstalledAuth) -> Result<String> {
    // スコープ付きでトークン取得を行う。
    let token = authn.token(&auth::scopes()).await?;
    // アクセストークン文字列を取り出す。
    let token = token.token().ok_or_else(|| anyhow!("no access token"))?;
    Ok(token.to_string())
}

/// シートへ値を書き込み、PDFをエクスポートしてDriveへアップロードする。
async fn commit_one(
    http: &Client,
    authn: &auth::InstalledAuth,
    cfg: &Config,
    fields: &ReceiptFields,
    target_month_ym: &str,
    tx: &mpsc::Sender<WorkerEvent>,
    job_id: uuid::Uuid,
) -> Result<()> {
    // 必須IDが揃っているかを事前確認する。
    if cfg.google.template_sheet_id.is_empty() || cfg.google.output_folder_id.is_empty() {
        return Err(anyhow!("template_sheet_id / output_folder_id is not set"));
    }

    // 一連の処理で使うアクセストークンを取得する。
    let token = access_token(authn).await?;

    // シート名は空白を除去して安定した名前にする。
    let safe_name = cfg.user.full_name.replace(' ', "");
    let new_sheet_name = format!(
        "立替経費精算書_{}_{}",
        target_month_ym.replace('-', ""),
        safe_name
    );
    // テンプレートがショートカットなら実体IDへ解決する。
    let template_sheet_id =
        drive::resolve_sheet_id(http, &token, &cfg.google.template_sheet_id).await?;
    // テンプレートをコピーして新しいシートファイルを作成する。
    let copied_sheet_id =
        drive::copy_file(http, &token, &template_sheet_id, &new_sheet_name, None).await?;

    // A1レンジを作るために最初のシート名を取得する。
    let (sheet_title, _rows) =
        sheets::get_first_sheet_title_and_rows(http, &token, &copied_sheet_id).await?;

    // ヘッダー（氏名・対象月）を埋める。
    let month_date = format!("{}-01", target_month_ym);
    let mut updates: Vec<(String, Vec<Vec<serde_json::Value>>)> = vec![];

    // 氏名セルの更新。
    updates.push((
        format!("{}!{}", sheet_title, cfg.template.name_cell),
        vec![vec![serde_json::Value::String(cfg.user.full_name.clone())]],
    ));
    // 対象月セルの更新。
    updates.push((
        format!("{}!{}", sheet_title, cfg.template.target_month_cell),
        vec![vec![serde_json::Value::String(month_date)]],
    ));

    // 経費テーブル内の次の空行を探す。
    let existing = sheets::count_existing_rows_in_col(
        http,
        &token,
        &copied_sheet_id,
        &sheet_title,
        &cfg.general_expense.date_col,
        cfg.general_expense.start_row,
    )
    .await?;

    // 追加する行番号を算出する。
    let row = cfg.general_expense.start_row + existing;

    // 領収書1行分の書き込みレンジを作る。
    let range = format!(
        "{}!{}{}:{}{}",
        sheet_title, cfg.general_expense.date_col, row, cfg.general_expense.note_col, row
    );

    // 1行分の値を更新リストへ追加する。
    updates.push((
        range,
        vec![vec![
            serde_json::Value::String(fields.date_ymd.clone()),
            serde_json::Value::String(fields.reason.clone()),
            serde_json::Value::Number(fields.amount_yen.into()),
            serde_json::Value::String(fields.category.clone()),
            serde_json::Value::String(fields.note.clone()),
        ]],
    ));

    // まとめてバッチ更新する。
    sheets::values_batch_update(http, &token, &copied_sheet_id, updates).await?;

    // PDFエクスポートとアップロードを実行する。
    let _ = tx
        .send(WorkerEvent::JobUpdated {
            job_id,
            status: JobStatus::ExportingPdf,
        })
        .await;

    let pdf = drive::export_pdf(http, &token, &copied_sheet_id).await?;

    // PDFアップロード中にステータスを更新する。
    let _ = tx
        .send(WorkerEvent::JobUpdated {
            job_id,
            status: JobStatus::UploadingPdf,
        })
        .await;

    // PDFのファイル名を組み立てる。
    let pdf_name = format!("{}_立替経費精算書_{}.pdf", target_month_ym, safe_name);
    // Driveへアップロードして完了させる。
    let _pdf_file_id =
        drive::upload_pdf(http, &token, &cfg.google.output_folder_id, &pdf_name, pdf).await?;

    Ok(())
}
