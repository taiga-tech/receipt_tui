//! ショートカット設定の管理。

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// ショートカット設定の全体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    pub main: MainShortcuts,
    pub settings: SettingsShortcuts,
    pub edit_job: EditJobShortcuts,
    pub wizard: WizardShortcuts,
    pub input_box: InputBoxShortcuts,
}

/// メイン画面のショートカット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainShortcuts {
    pub quit: String,
    pub settings: String,
    pub refresh: String,
    pub enter: String,
    pub down: String,
    pub up: String,
}

/// 設定画面のショートカット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsShortcuts {
    pub cancel: String,
    pub save: String,
    pub input_folder: String,
    pub output_folder: String,
    pub template: String,
    pub name: String,
}

/// 編集画面のショートカット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditJobShortcuts {
    pub cancel: String,
    pub next_field: String,
    pub commit: String,
    pub target_month: String,
    pub edit_field: String,
}

/// ウィザード画面のショートカット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardShortcuts {
    pub proceed: String,
    pub skip: String,
}

/// InputBoxのショートカット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBoxShortcuts {
    pub confirm: String,
    pub cancel: String,
    pub backspace: String,
    pub delete: String,
    pub left: String,
    pub right: String,
    pub home: String,
    pub end: String,
    pub clear_line: String,
}

impl Shortcuts {
    /// TOMLから読み込み、無ければデフォルトを返す。
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            // 既存ファイルを読み込んでパースする。
            let content = std::fs::read_to_string(path)?;
            let shortcuts: Shortcuts = toml::from_str(&content)?;
            Ok(shortcuts)
        } else {
            // 未作成の場合は既定値を利用する。
            Ok(Self::default())
        }
    }

    /// TOMLとして保存する。
    #[allow(dead_code)]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // 文字列にシリアライズする。
        let content = toml::to_string_pretty(self)?;
        // ファイルへ書き込む。
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            main: MainShortcuts {
                quit: "q".into(),
                settings: "t".into(),
                refresh: "r".into(),
                enter: "Enter".into(),
                down: "Down".into(),
                up: "Up".into(),
            },
            settings: SettingsShortcuts {
                cancel: "Esc".into(),
                save: "Enter".into(),
                input_folder: "i".into(),
                output_folder: "o".into(),
                template: "p".into(),
                name: "n".into(),
            },
            edit_job: EditJobShortcuts {
                cancel: "Esc".into(),
                next_field: "Tab".into(),
                commit: "Enter".into(),
                target_month: "m".into(),
                edit_field: "e".into(),
            },
            wizard: WizardShortcuts {
                proceed: "Enter".into(),
                skip: "Esc".into(),
            },
            input_box: InputBoxShortcuts {
                confirm: "Enter".into(),
                cancel: "Esc".into(),
                backspace: "Backspace".into(),
                delete: "Delete".into(),
                left: "Left".into(),
                right: "Right".into(),
                home: "Home".into(),
                end: "End".into(),
                clear_line: "Ctrl+u".into(),
            },
        }
    }
}

/// KeyEventがショートカット文字列と一致するか判定する。
pub fn matches_shortcut(key: &KeyEvent, shortcut: &str) -> bool {
    // ショートカット文字列を分解する（例: "Ctrl+u", "a", "Enter"）。
    let parts: Vec<&str> = shortcut.split('+').collect();

    let (modifiers_str, key_str) = if parts.len() > 1 {
        // 修飾キー付きの形式（例: "Ctrl+u"）。
        (&parts[0..parts.len() - 1], parts[parts.len() - 1])
    } else {
        // 修飾キーなしの形式（例: "a", "Enter"）。
        (&[][..], parts[0])
    };

    // 修飾キーを解析して期待値を作る。
    let mut expected_modifiers = KeyModifiers::empty();
    for modifier in modifiers_str {
        match *modifier {
            "Ctrl" | "ctrl" => expected_modifiers |= KeyModifiers::CONTROL,
            "Alt" | "alt" => expected_modifiers |= KeyModifiers::ALT,
            "Shift" | "shift" => expected_modifiers |= KeyModifiers::SHIFT,
            _ => return false,
        }
    }

    // 修飾キーが一致しなければ即座に不一致とする。
    if key.modifiers != expected_modifiers {
        return false;
    }

    // キーコードの種別ごとに一致判定を行う。
    match key_str {
        "Enter" | "enter" => key.code == KeyCode::Enter,
        "Esc" | "esc" => key.code == KeyCode::Esc,
        "Tab" | "tab" => key.code == KeyCode::Tab,
        "Backspace" | "backspace" => key.code == KeyCode::Backspace,
        "Delete" | "delete" => key.code == KeyCode::Delete,
        "Up" | "up" => key.code == KeyCode::Up,
        "Down" | "down" => key.code == KeyCode::Down,
        "Left" | "left" => key.code == KeyCode::Left,
        "Right" | "right" => key.code == KeyCode::Right,
        "Home" | "home" => key.code == KeyCode::Home,
        "End" | "end" => key.code == KeyCode::End,
        // 単一文字は Char として比較する。
        s if s.len() == 1 => {
            if let Some(c) = s.chars().next() {
                key.code == KeyCode::Char(c)
            } else {
                false
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_shortcut_simple_char() {
        // 単一文字の一致判定を検証する。
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert!(matches_shortcut(&key, "q"));
        assert!(!matches_shortcut(&key, "w"));
    }

    #[test]
    fn test_matches_shortcut_special_key() {
        // 特殊キーの一致判定を検証する。
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert!(matches_shortcut(&key, "Enter"));
        assert!(!matches_shortcut(&key, "Esc"));
    }

    #[test]
    fn test_matches_shortcut_with_modifier() {
        // 修飾キー付きの一致判定を検証する。
        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert!(matches_shortcut(&key, "Ctrl+u"));
        assert!(!matches_shortcut(&key, "u"));
    }

    #[test]
    fn test_matches_shortcut_arrow_keys() {
        // 矢印キーの一致判定を検証する。
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        assert!(matches_shortcut(&key, "Up"));
        assert!(!matches_shortcut(&key, "Down"));
    }
}
