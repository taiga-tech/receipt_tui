//! アプリケーションのエントリポイントとランタイム初期化。

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;

mod app;
mod config;
mod events;
mod google;
mod input;
mod jobs;
mod layout;
mod shortcuts;
mod ui;
mod wizard;
mod worker;

/// ファイルロギングを初期化し、非同期ガードを生存させる。
fn init_logging() -> Result<WorkerGuard> {
    // ログ出力先ファイル名を決める。
    let log_file = "receipt_tui.log";
    // TUIの標準出力を汚さないよう、ファイルへ直接書き込む。
    let file_appender = tracing_appender::rolling::never(".", log_file);
    // 非同期書き込み用のラッパーとガードを用意する。
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // フォーマッタと出力先を設定して初期化する。
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to init logging: {e}"))?;
    // ログの保存先を通知しておく。
    tracing::info!("logging to {}", log_file);
    Ok(guard)
}

#[tokio::main]
/// エントリポイント：ログ初期化→UI開始→端末復元。
async fn main() -> Result<()> {
    // ロガーを初期化し、ガードを保持して書き込みを継続させる。
    let _log_guard = init_logging()?;
    // 起動ログを出力する。
    tracing::info!("app starting");
    // TUI用の端末状態へ切り替える。
    let mut terminal = ui::init_terminal()?;
    // メインアプリを実行する。
    let res = app::run_app(&mut terminal).await;
    // 端末の状態を必ず元に戻す。
    ui::restore_terminal()?;
    // エラーがあればログに残す。
    if let Err(ref e) = res {
        tracing::error!("app error: {e}");
    }
    // 終了ログを出力する。
    tracing::info!("app exiting");
    res
}
