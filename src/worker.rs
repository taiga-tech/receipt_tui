//! Background worker handling Google API jobs.

use crate::{
    config::Config,
    google::{auth, drive, sheets},
    jobs::{Job, JobStatus, ReceiptFields},
};
use anyhow::{Result, anyhow};
use reqwest::Client;
use tokio::sync::mpsc;

/// Commands sent from the UI to the worker.
#[derive(Debug)]
pub enum WorkerCmd {
    /// Re-scan Drive for input images.
    RefreshJobs,
    /// Persist and apply updated settings.
    SaveSettings(Config),
    /// Write the edited fields and export/upload the PDF.
    CommitJobEdits {
        job_id: uuid::Uuid,
        fields: ReceiptFields,
        target_month_ym: String,
    },
}

/// Events emitted by the worker for UI updates.
#[derive(Clone, Debug)]
pub enum WorkerEvent {
    /// Full job list loaded from Drive.
    JobsLoaded(Vec<Job>),
    /// Single job status update.
    JobUpdated {
        job_id: uuid::Uuid,
        status: JobStatus,
    },
    /// Informational log message.
    Log(String),
    /// User-visible error message.
    Error(String),
}

/// Main worker loop: authenticate, then handle commands sequentially.
pub async fn run(
    mut rx: mpsc::Receiver<WorkerCmd>,
    tx: mpsc::Sender<WorkerEvent>,
    mut cfg: Config,
) {
    // Shared HTTP client for all API calls.
    let http = Client::new();
    tracing::info!("worker started");

    // OAuth setup is done once; failures are terminal.
    let authn = match auth::authenticator().await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("OAuth init failed: {e}");
            let _ = tx
                .send(WorkerEvent::Error(format!("OAuth init failed: {e}")))
                .await;
            return;
        }
    };
    tracing::info!("OAuth authenticator ready");

    // Process commands one at a time to keep state consistent.
    while let Some(cmd) = rx.recv().await {
        match cmd {
            WorkerCmd::SaveSettings(new_cfg) => {
                tracing::info!("settings updated");
                cfg = new_cfg;
                let _ = tx.send(WorkerEvent::Log("settings updated".into())).await;
            }

            WorkerCmd::RefreshJobs => {
                tracing::info!("refresh jobs");
                // Ensure minimal config before touching Drive.
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
                        // List image files and map them into editable jobs.
                        match drive::list_images_in_folder(
                            &http,
                            &token,
                            &cfg.google.input_folder_id,
                        )
                        .await
                        {
                            Ok(files) => {
                                tracing::info!("drive list success: {} files", files.len());
                                let jobs = files
                                    .into_iter()
                                    .map(|f| {
                                        let mut j = Job::new(f.id, f.name);
                                        // Jobs start in editable state to allow user input.
                                        j.status = JobStatus::WaitingUserFix;
                                        j
                                    })
                                    .collect::<Vec<_>>();
                                let _ = tx.send(WorkerEvent::JobsLoaded(jobs)).await;
                            }
                            Err(e) => {
                                tracing::error!("drive list failed: {e}");
                                let _ = tx
                                    .send(WorkerEvent::Error(format!("list failed: {e}")))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("token failed: {e}");
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
                // Update status so the UI shows progress immediately.
                let _ = tx
                    .send(WorkerEvent::JobUpdated {
                        job_id,
                        status: JobStatus::WritingSheet,
                    })
                    .await;

                let r =
                    commit_one(&http, &authn, &cfg, &fields, &target_month_ym, &tx, job_id).await;
                match r {
                    Ok(_) => {
                        tracing::info!("commit job done: {job_id}");
                        let _ = tx
                            .send(WorkerEvent::JobUpdated {
                                job_id,
                                status: JobStatus::Done,
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::error!("commit job failed: {job_id}: {e}");
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

/// Fetch a fresh access token from the authenticator.
async fn access_token(authn: &auth::InstalledAuth) -> Result<String> {
    let token = authn.token(&auth::scopes()).await?;
    let token = token.token().ok_or_else(|| anyhow!("no access token"))?;
    Ok(token.to_string())
}

/// Write fields to a copied sheet, then export and upload a PDF.
async fn commit_one(
    http: &Client,
    authn: &auth::InstalledAuth,
    cfg: &Config,
    fields: &ReceiptFields,
    target_month_ym: &str,
    tx: &mpsc::Sender<WorkerEvent>,
    job_id: uuid::Uuid,
) -> Result<()> {
    // Ensure required IDs are available before making API calls.
    if cfg.google.template_sheet_id.is_empty() || cfg.google.output_folder_id.is_empty() {
        return Err(anyhow!("template_sheet_id / output_folder_id is not set"));
    }

    // Resolve a valid access token once for the whole workflow.
    let token = access_token(authn).await?;

    // Build a sheet name that is stable and avoids spaces.
    let safe_name = cfg.user.full_name.replace(' ', "");
    let new_sheet_name = format!(
        "立替経費精算書_{}_{}",
        target_month_ym.replace('-', ""),
        safe_name
    );
    // Resolve template shortcut to the actual sheet id if needed.
    let template_sheet_id =
        drive::resolve_sheet_id(http, &token, &cfg.google.template_sheet_id).await?;
    // Copy the template into a new sheet file.
    let copied_sheet_id =
        drive::copy_file(http, &token, &template_sheet_id, &new_sheet_name, None).await?;

    // Read the first sheet title for range construction.
    let (sheet_title, _rows) =
        sheets::get_first_sheet_title_and_rows(http, &token, &copied_sheet_id).await?;

    // Fill in the header fields (name + target month).
    let month_date = format!("{}-01", target_month_ym);
    let mut updates: Vec<(String, Vec<Vec<serde_json::Value>>)> = vec![];

    updates.push((
        format!("{}!{}", sheet_title, cfg.template.name_cell),
        vec![vec![serde_json::Value::String(cfg.user.full_name.clone())]],
    ));
    updates.push((
        format!("{}!{}", sheet_title, cfg.template.target_month_cell),
        vec![vec![serde_json::Value::String(month_date)]],
    ));

    // Find the next empty row in the expense table.
    let existing = sheets::count_existing_rows_in_col(
        http,
        &token,
        &copied_sheet_id,
        &sheet_title,
        &cfg.general_expense.date_col,
        cfg.general_expense.start_row,
    )
    .await?;

    let row = cfg.general_expense.start_row + existing;

    // Write one row of receipt values.
    let range = format!(
        "{}!{}{}:{}{}",
        sheet_title, cfg.general_expense.date_col, row, cfg.general_expense.note_col, row
    );

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

    // Apply all updates in one batch request.
    sheets::values_batch_update(http, &token, &copied_sheet_id, updates).await?;

    // Export the sheet and upload the PDF to Drive.
    let _ = tx
        .send(WorkerEvent::JobUpdated {
            job_id,
            status: JobStatus::ExportingPdf,
        })
        .await;

    let pdf = drive::export_pdf(http, &token, &copied_sheet_id).await?;

    let _ = tx
        .send(WorkerEvent::JobUpdated {
            job_id,
            status: JobStatus::UploadingPdf,
        })
        .await;

    let pdf_name = format!("{}_立替経費精算書_{}.pdf", target_month_ym, safe_name);
    let _pdf_file_id =
        drive::upload_pdf(http, &token, &cfg.google.output_folder_id, &pdf_name, pdf).await?;

    Ok(())
}
