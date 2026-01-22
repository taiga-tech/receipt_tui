//! レイアウト計算のヘルパー関数。

use ratatui::prelude::*;

/// メインレイアウトの4つの領域。
pub struct MainLayout {
    /// ジョブテーブル + 情報パネルの領域。
    pub body: Rect,
    /// ヘルプバーの領域。
    pub help_bar: Rect,
    /// ステータスバーの領域。
    pub status_bar: Rect,
}

/// ボディ部の2つの領域（ジョブテーブル + 情報パネル）。
pub struct BodyLayout {
    /// ジョブテーブルの領域。
    pub jobs_table: Rect,
    /// 情報パネルの領域。
    pub info_panel: Rect,
}

/// メイン画面を4つの領域に分割（本文 + ヘルプ + ステータス）。
pub fn create_main_layout(area: Rect) -> MainLayout {
    // 縦方向にレイアウトを分割する。
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // 本文（ジョブテーブル + 情報パネル）
            Constraint::Length(3), // ヘルプバー
            Constraint::Length(3), // ステータスバー
        ])
        .split(area);

    // 分割結果を構造体に詰めて返す。
    MainLayout {
        body: chunks[0],
        help_bar: chunks[1],
        status_bar: chunks[2],
    }
}

/// 本文領域を2つに分割（ジョブテーブル70% + 情報パネル30%）。
pub fn create_body_layout(area: Rect) -> BodyLayout {
    // 横方向に2カラムへ分割する。
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // ジョブテーブル
            Constraint::Percentage(30), // 情報パネル
        ])
        .split(area);

    // 分割結果を構造体に詰めて返す。
    BodyLayout {
        jobs_table: chunks[0],
        info_panel: chunks[1],
    }
}
