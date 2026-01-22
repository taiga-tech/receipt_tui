//! TUIのイベントループ、入力処理、状態管理。

mod handlers;
mod render;

use anyhow::Result;
use chrono::Datelike;
use crossterm::event::{self, Event};
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc;

use crate::{
    config::Config,
    events::{Screen, UiState},
    input::InputBoxState,
    jobs::Job,
    shortcuts::Shortcuts,
    ui::Tui,
    wizard,
    worker::{self, WorkerCmd, WorkerEvent},
};

use handlers::{handle_key, is_ctrl_c};
use render::draw;

/// 入力処理と描画で共有するアプリ状態。
pub struct App {
    /// 永続化された設定ファイルのパス。
    pub cfg_path: PathBuf,
    /// メモリ上の現在設定。
    pub cfg: Config,
    /// 選択位置やステータスなどUI固有の状態。
    pub ui: UiState,
    /// Driveから読み込んだジョブ（画像1件につき1ジョブ）。
    pub jobs: Vec<Job>,
    /// Workerへのコマンド送信チャネル。
    pub worker_tx: mpsc::Sender<WorkerCmd>,
    /// Workerからのイベント受信チャネル。
    pub worker_rx: mpsc::Receiver<WorkerEvent>,

    /// 設定画面で編集する入力フォルダID。
    pub in_folder: String,
    /// 設定画面で編集する出力フォルダID。
    pub out_folder: String,
    /// 設定画面で編集するテンプレートシートID。
    pub template_id: String,
    /// 設定画面で編集する氏名。
    pub full_name: String,

    /// 領収書行を追加する対象月（YYYY-MM）。
    pub edit_target_month: String,

    /// 入力ボックスの状態（入力中はSome）。
    pub input_box: Option<InputBoxState>,

    /// 初期設定ウィザードの状態。
    pub wizard_state: wizard::WizardState,

    /// ショートカットキー設定。
    pub shortcuts: Shortcuts,
}

/// ユーザーが終了するまでメインTUIループを回す。
pub async fn run_app(terminal: &mut Tui) -> Result<()> {
    // 設定ファイルを読み込む（初回はデフォルトを生成）。
    let cfg_path = PathBuf::from("config.toml");
    let cfg = Config::load_or_default(&cfg_path)?;

    // ショートカット設定を読み込む（無ければデフォルト）。
    let shortcuts_path = PathBuf::from("shortcut.toml");
    let shortcuts = Shortcuts::load_or_default(&shortcuts_path)?;

    // Worker通信用のコマンド/イベントチャネルを作る。
    let (tx_cmd, rx_cmd) = mpsc::channel::<WorkerCmd>(64);
    let (tx_ev, rx_ev) = mpsc::channel::<WorkerEvent>(256);

    // 初期設定スナップショットでWorkerを起動する。
    tokio::spawn(worker::run(rx_cmd, tx_ev, cfg.clone()));

    // 設定の充足度に応じて初期画面を決める。
    let initial_screen = if needs_initial_setup(&cfg) {
        Screen::InitialSetup
    } else {
        Screen::Main
    };

    // 現在日時から編集対象月を自動生成する。
    let now = chrono::Local::now();
    let edit_target_month = format!("{}-{:02}", now.year(), now.month());

    // アプリ状態を初期化する。
    let mut app = App {
        cfg_path,
        cfg: cfg.clone(),
        ui: UiState {
            screen: initial_screen.clone(),
            selected: 0,
            log: vec![],
            status: "Ready".into(),
            editing_field_idx: 0,
            error: None,
        },
        jobs: vec![],
        worker_tx: tx_cmd,
        worker_rx: rx_ev,
        in_folder: cfg.google.input_folder_id.clone(),
        out_folder: cfg.google.output_folder_id.clone(),
        template_id: cfg.google.template_sheet_id.clone(),
        full_name: cfg.user.full_name.clone(),
        edit_target_month,
        input_box: None,
        wizard_state: wizard::WizardState::new(),
        shortcuts,
    };

    // ウィザード以外なら起動時に一覧を更新する。
    if initial_screen == Screen::Main {
        request_refresh(&mut app).await?;
    }

    loop {
        // 現在の状態を描画する。
        terminal.draw(|f| draw(f, &app))?;

        // 入力処理の前にWorkerイベントを消化する。
        while let Ok(ev) = app.worker_rx.try_recv() {
            handle_worker_event(&mut app, ev)?;
        }

        // UIの応答性確保のため短いタイムアウトで入力をポーリングする。
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(k) = event::read()?
        {
            // どのフェーズでもCtrl+Cで終了できるようにする。
            if is_ctrl_c(&k) {
                break;
            }
            if handle_key(&mut app, k).await? {
                break;
            }
        }
    }
    Ok(())
}

/// WorkerイベントをUI状態へ反映する。
fn handle_worker_event(app: &mut App, ev: WorkerEvent) -> Result<()> {
    match ev {
        WorkerEvent::JobsLoaded(jobs) => {
            // ジョブ一覧を更新し選択を先頭に戻す。
            app.jobs = jobs;
            app.ui.selected = 0;
            app.ui.status = format!("Loaded {} jobs", app.jobs.len());
        }
        WorkerEvent::JobUpdated { job_id, status } => {
            // 対象ジョブの状態を更新する。
            if let Some(j) = app.jobs.iter_mut().find(|j| j.id == job_id) {
                j.status = status;
            }
        }
        WorkerEvent::Log(s) => {
            // ログを追加する。
            app.ui.log.push(s);
        }
        WorkerEvent::Error(s) => {
            // ステータスにエラーを表示する。
            app.ui.status = format!("Error: {s}");
        }
    }
    Ok(())
}

/// 必須設定が揃っていればWorkerへリフレッシュ要求する。
pub async fn request_refresh(app: &mut App) -> Result<()> {
    // 必須IDが未設定なら案内メッセージを出す。
    if app.cfg.google.input_folder_id.is_empty()
        || app.cfg.google.output_folder_id.is_empty()
        || app.cfg.google.template_sheet_id.is_empty()
    {
        app.ui.status = "Settings required (press t)".into();
        tracing::warn!("refresh skipped: settings required");
    } else {
        // Workerへリフレッシュを依頼する。
        tracing::info!("refresh requested");
        app.worker_tx.send(WorkerCmd::RefreshJobs).await?;
        app.ui.status = "Refreshing jobs...".into();
    }
    Ok(())
}

/// 初期設定ウィザードが必要か判定する。
fn needs_initial_setup(cfg: &Config) -> bool {
    // いずれかの必須項目が未入力ならウィザード対象。
    cfg.google.input_folder_id.is_empty()
        || cfg.google.output_folder_id.is_empty()
        || cfg.google.template_sheet_id.is_empty()
        || cfg.user.full_name == "Your Name"
}
