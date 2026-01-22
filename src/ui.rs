//! TUI用端末の初期化と復元。

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout};

/// アプリ全体で使う端末型。
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// 代替画面へ切り替え、rawモードを有効化する。
pub fn init_terminal() -> Result<Tui> {
    // キー入力を即時に受け取れるようrawモードへ切り替える。
    enable_raw_mode()?;
    // 標準出力を取得して代替画面へ入る。
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // CrosstermバックエンドでTerminalを構築する。
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

/// 終了時に端末状態を元に戻す。
pub fn restore_terminal() -> Result<()> {
    // rawモードを解除する。
    disable_raw_mode()?;
    // 代替画面を終了して元の画面へ戻す。
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
