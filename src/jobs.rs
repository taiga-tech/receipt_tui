//! ジョブと領収書入力項目のモデル。

use uuid::Uuid;

/// 1行分の領収書入力項目。
#[derive(Clone, Debug, Default)]
pub struct ReceiptFields {
    /// 支払日（ISO形式: YYYY-MM-DD）。
    pub date_ymd: String, // "2025-12-19"
    /// 用途/摘要。
    pub reason: String,
    /// 金額（円）。
    pub amount_yen: i64,
    /// テンプレートが期待する勘定科目。
    pub category: String,
    /// 備考（任意）。
    pub note: String,
}

/// Worker内の処理進行に応じたジョブ状態。
#[derive(Clone, Debug)]
pub enum JobStatus {
    /// 処理待ち。
    Queued,
    /// ユーザー編集待ち。
    WaitingUserFix,
    /// スプレッドシートへの書き込み中。
    WritingSheet,
    /// シートをPDFにエクスポート中。
    ExportingPdf,
    /// PDFをDriveへアップロード中。
    UploadingPdf,
    /// 正常完了。
    Done,
    /// 失敗（エラーメッセージ付き）。
    Error(String),
}

/// Drive上の画像1件とその処理状態。
#[derive(Clone, Debug)]
pub struct Job {
    /// 状態更新に使う安定ID。
    pub id: Uuid,
    /// 画像ファイルのDrive ID。
    pub drive_file_id: String,
    /// 表示用のファイル名。
    pub filename: String,
    /// 現在の処理状態。
    pub status: JobStatus,
    /// ユーザー入力の編集項目。
    pub fields: ReceiptFields,
}

impl Job {
    /// デフォルト入力値と待機状態でジョブを作成する。
    pub fn new(drive_file_id: String, filename: String) -> Self {
        // 新しいUUIDを発行して安定IDとする。
        Self {
            id: Uuid::new_v4(),
            // 受け取ったDrive情報をセットする。
            drive_file_id,
            filename,
            // 初期状態は待機。
            status: JobStatus::Queued,
            // 入力項目はデフォルトで初期化する。
            fields: ReceiptFields::default(),
        }
    }
}
