//! Job and receipt field models.

use uuid::Uuid;

/// Editable fields for a single receipt row.
#[derive(Clone, Debug, Default)]
pub struct ReceiptFields {
    /// Expense date in ISO format (YYYY-MM-DD).
    pub date_ymd: String, // "2025-12-19"
    /// Expense reason/description.
    pub reason: String,
    /// Amount in yen.
    pub amount_yen: i64,
    /// Category name as expected by the template.
    pub category: String,
    /// Optional note.
    pub note: String,
}

/// Lifecycle state for a job as it progresses through the worker.
#[derive(Clone, Debug)]
pub enum JobStatus {
    /// Waiting to be processed.
    Queued,
    /// Waiting for user to edit fields.
    WaitingUserFix,
    /// Writing values into the spreadsheet.
    WritingSheet,
    /// Exporting the sheet to PDF.
    ExportingPdf,
    /// Uploading the PDF to Drive.
    UploadingPdf,
    /// Completed successfully.
    Done,
    /// Failed with an error message.
    Error(String),
}

/// A single Drive image and its processing status.
#[derive(Clone, Debug)]
pub struct Job {
    /// Stable id used to update status.
    pub id: Uuid,
    /// Drive file id for the source image.
    pub drive_file_id: String,
    /// Display name of the file.
    pub filename: String,
    /// Current processing state.
    pub status: JobStatus,
    /// Editable fields captured from the user.
    pub fields: ReceiptFields,
}

impl Job {
    /// Create a new job with default fields and queued status.
    pub fn new(drive_file_id: String, filename: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            drive_file_id,
            filename,
            status: JobStatus::Queued,
            fields: ReceiptFields::default(),
        }
    }
}
