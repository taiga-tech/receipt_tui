//! レイアウト計算のヘルパー関数

use ratatui::prelude::*;

/// メインレイアウトの4つの領域
pub struct MainLayout {
    /// Jobs Table + INFO Panelの領域
    pub body: Rect,
    /// HELPバーの領域
    pub help_bar: Rect,
    /// STATUSバーの領域
    pub status_bar: Rect,
}

/// ボディ部の2つの領域（Jobs Table + INFO Panel）
pub struct BodyLayout {
    /// Jobs Tableの領域
    pub jobs_table: Rect,
    /// INFO Panelの領域
    pub info_panel: Rect,
}

/// メイン画面を4つの領域に分割（Body + HELP + STATUS）
pub fn create_main_layout(area: Rect) -> MainLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Body（Jobs Table + INFO Panel）
            Constraint::Length(3), // HELPバー
            Constraint::Length(3), // STATUSバー
        ])
        .split(area);

    MainLayout {
        body: chunks[0],
        help_bar: chunks[1],
        status_bar: chunks[2],
    }
}

/// Body領域を2つに分割（Jobs Table 70% + INFO Panel 30%）
pub fn create_body_layout(area: Rect) -> BodyLayout {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Jobs Table
            Constraint::Percentage(30), // INFO Panel
        ])
        .split(area);

    BodyLayout {
        jobs_table: chunks[0],
        info_panel: chunks[1],
    }
}
