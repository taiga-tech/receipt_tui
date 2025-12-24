//! TUI内での文字列入力コンポーネント（InputBox）

use ratatui::{
    layout::Alignment,
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

/// InputBox入力状態
#[derive(Clone, Debug)]
pub struct InputBoxState {
    /// プロンプトメッセージ
    pub prompt: String,
    /// 現在の入力値
    pub value: String,
    /// カーソル位置（文字単位）
    pub cursor: usize,
    /// 入力完了時のコールバック識別子
    pub callback_id: InputCallbackId,
}

/// 入力完了時のコールバック識別子
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputCallbackId {
    // Settings画面用
    SettingsInputFolder,
    SettingsOutputFolder,
    SettingsTemplateId,
    SettingsFullName,

    // EditJob画面用
    EditTargetMonth,
    EditJobField(usize), // 0..4

    // Wizard画面用
    WizardInputFolder,
    WizardOutputFolder,
    WizardTemplateId,
    WizardFullName,
}

impl InputBoxState {
    /// 文字を挿入
    pub fn insert_char(&mut self, c: char) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut new_chars = chars[..self.cursor].to_vec();
        new_chars.push(c);
        new_chars.extend_from_slice(&chars[self.cursor..]);
        self.value = new_chars.iter().collect();
        self.cursor += 1;
    }

    /// Backspace（カーソル前の文字を削除）
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let chars: Vec<char> = self.value.chars().collect();
            self.value = chars
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != self.cursor - 1)
                .map(|(_, c)| c)
                .collect();
            self.cursor -= 1;
        }
    }

    /// Delete（カーソル位置の文字を削除）
    pub fn delete(&mut self) {
        let char_count = self.value.chars().count();
        if self.cursor < char_count {
            let chars: Vec<char> = self.value.chars().collect();
            self.value = chars
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != self.cursor)
                .map(|(_, c)| c)
                .collect();
        }
    }

    /// カーソルを左に移動
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// カーソルを右に移動
    pub fn move_right(&mut self) {
        let char_count = self.value.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
        }
    }

    /// カーソルを先頭に移動
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// カーソルを末尾に移動
    pub fn move_end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    /// 行全体をクリア
    pub fn clear_line(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }
}

/// InputBoxをポップアップとして描画
pub fn render_input_box(f: &mut Frame, state: &InputBoxState) {
    // 中央に配置されたポップアップ領域を計算
    let popup_area = centered_popup(f.area(), 70, 7);

    // ポップアップの背景をクリア
    f.render_widget(Clear, popup_area);

    // ポップアップのボーダー
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Input")
        .style(Style::default().bg(Color::DarkGray));
    f.render_widget(block, popup_area);

    // 内部レイアウト（プロンプト + 入力フィールド + ヘルプ）
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // プロンプト
            Constraint::Length(1), // 入力フィールド
            Constraint::Length(1), // 空行
            Constraint::Length(1), // ヘルプ
        ])
        .split(popup_area);

    // プロンプトメッセージ
    let prompt_widget = Paragraph::new(state.prompt.clone()).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(prompt_widget, inner_layout[0]);

    // 入力値の表示（横スクロール対応）
    let display_width = inner_layout[1].width as usize;
    let scroll_offset = if state.cursor > display_width.saturating_sub(2) {
        state.cursor.saturating_sub(display_width - 2)
    } else {
        0
    };

    let chars: Vec<char> = state.value.chars().collect();
    let visible_text: String = chars
        .iter()
        .skip(scroll_offset)
        .take(display_width)
        .collect();

    // カーソル位置を視覚的に表現（|を挿入）
    let cursor_pos_in_visible = state.cursor.saturating_sub(scroll_offset);
    let visible_with_cursor = if cursor_pos_in_visible <= visible_text.chars().count() {
        let visible_chars: Vec<char> = visible_text.chars().collect();
        let before: String = visible_chars[..cursor_pos_in_visible.min(visible_chars.len())]
            .iter()
            .collect();
        let after: String = visible_chars[cursor_pos_in_visible.min(visible_chars.len())..]
            .iter()
            .collect();
        format!("{}|{}", before, after)
    } else {
        format!("{}|", visible_text)
    };

    let input_widget = Paragraph::new(visible_with_cursor).style(Style::default().fg(Color::Green));
    f.render_widget(input_widget, inner_layout[1]);

    // ヘルプテキスト
    let help = Paragraph::new("Enter=確定 | ESC=キャンセル | Ctrl+U=クリア")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(help, inner_layout[3]);
}

/// 中央配置のポップアップ領域を計算
fn centered_popup(area: Rect, width_percent: u16, height: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup_layout[1])[1]
}
