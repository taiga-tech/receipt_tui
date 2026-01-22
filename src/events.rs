//! 画面遷移用のUI状態と画面種別。

/// TUIで現在表示中の画面。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// メインのジョブ一覧画面。
    Main,
    /// 設定編集画面。
    Settings,
    /// 選択ジョブの編集画面。
    EditJob,
    /// 初期設定ウィザード画面。
    InitialSetup,
}

/// 描画側と共有するUI状態。
#[derive(Clone, Debug)]
pub struct UiState {
    /// 現在の画面。
    pub screen: Screen,
    /// ジョブ一覧の選択行。
    pub selected: usize,
    /// 右側パネルに表示するログ。
    pub log: Vec<String>,
    /// 画面下部のステータス文言。
    pub status: String,
    /// 編集対象のフィールド位置（0..4）。
    pub editing_field_idx: usize, // 0..4 の範囲
    /// エラーメッセージ（強調表示用）。
    pub error: Option<String>,
}
