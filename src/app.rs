//! TUIのイベントループ、入力処理、描画。

use anyhow::Result;
use chrono::Datelike;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
};
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc;

use crate::{
    config::Config,
    events::{Screen, UiState},
    input::{self, InputBoxState, InputCallbackId},
    jobs::{Job, JobStatus},
    layout,
    shortcuts::{self, Shortcuts},
    ui::Tui,
    wizard,
    worker::{self, WorkerCmd, WorkerEvent},
};

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
            && handle_key(&mut app, k).await?
        {
            break;
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
async fn request_refresh(app: &mut App) -> Result<()> {
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

/// キー入力を1件処理し、終了すべきならtrueを返す。
async fn handle_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // 入力ボックスが開いていれば最優先で処理する。
    if app.input_box.is_some() {
        return handle_input_box_key(app, k).await;
    }

    // 画面ごとのハンドラへ委譲する。
    match app.ui.screen {
        Screen::Main => handle_main_key(app, k).await,
        Screen::Settings => handle_settings_key(app, k).await,
        Screen::EditJob => handle_edit_job_key(app, k).await,
        Screen::InitialSetup => handle_wizard_key(app, k).await,
    }
}

/// メイン画面のキー処理。
async fn handle_main_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // メイン画面のショートカットを参照する。
    let sc = &app.shortcuts.main;

    if shortcuts::matches_shortcut(&k, &sc.quit) {
        return Ok(true);
    } else if shortcuts::matches_shortcut(&k, &sc.settings) {
        // 設定画面へ遷移し、編集バッファを更新する。
        reload_settings_buffers(app);
        app.ui.screen = Screen::Settings;
        app.ui.status = "Settings".into();
    } else if shortcuts::matches_shortcut(&k, &sc.refresh) {
        // ジョブ一覧の再取得を依頼する。
        request_refresh(app).await?;
    } else if shortcuts::matches_shortcut(&k, &sc.down) {
        // 次の行へ移動する。
        if app.ui.selected + 1 < app.jobs.len() {
            app.ui.selected += 1;
        }
    } else if shortcuts::matches_shortcut(&k, &sc.up) {
        // 前の行へ移動する。
        if app.ui.selected > 0 {
            app.ui.selected -= 1;
        }
    } else if shortcuts::matches_shortcut(&k, &sc.enter)
        && app.jobs.get(app.ui.selected).is_some()
    {
        // 編集画面へ遷移し、編集フィールドを先頭に戻す。
        app.ui.screen = Screen::EditJob;
        app.ui.editing_field_idx = 0;
    }

    Ok(false)
}

/// 設定画面のキー処理。
async fn handle_settings_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // 設定画面のショートカットを参照する。
    let sc = &app.shortcuts.settings;

    if shortcuts::matches_shortcut(&k, &sc.cancel) {
        // 変更を破棄してメイン画面へ戻る。
        reload_settings_buffers(app);
        app.ui.screen = Screen::Main;
    } else if shortcuts::matches_shortcut(&k, &sc.save) {
        // 編集バッファを設定へ反映する。
        app.cfg.google.input_folder_id = app.in_folder.clone();
        app.cfg.google.output_folder_id = app.out_folder.clone();
        app.cfg.google.template_sheet_id = app.template_id.clone();
        app.cfg.user.full_name = app.full_name.clone();
        // 設定ファイルを保存する。
        app.cfg.save(&app.cfg_path)?;

        // Workerにも設定更新を通知する。
        app.worker_tx
            .send(WorkerCmd::SaveSettings(app.cfg.clone()))
            .await?;
        // 画面状態を更新してメインへ戻る。
        app.ui.screen = Screen::Main;
        app.ui.status = "Saved settings".into();
    } else if shortcuts::matches_shortcut(&k, &sc.input_folder) {
        // 入力フォルダIDの入力ボックスを開く。
        app.input_box = Some(InputBoxState {
            prompt: "Input folder ID:".into(),
            value: app.in_folder.clone(),
            cursor: app.in_folder.chars().count(),
            callback_id: InputCallbackId::SettingsInputFolder,
        });
    } else if shortcuts::matches_shortcut(&k, &sc.output_folder) {
        // 出力フォルダIDの入力ボックスを開く。
        app.input_box = Some(InputBoxState {
            prompt: "Output folder ID:".into(),
            value: app.out_folder.clone(),
            cursor: app.out_folder.chars().count(),
            callback_id: InputCallbackId::SettingsOutputFolder,
        });
    } else if shortcuts::matches_shortcut(&k, &sc.template) {
        // テンプレートシートIDの入力ボックスを開く。
        app.input_box = Some(InputBoxState {
            prompt: "Template sheet ID:".into(),
            value: app.template_id.clone(),
            cursor: app.template_id.chars().count(),
            callback_id: InputCallbackId::SettingsTemplateId,
        });
    } else if shortcuts::matches_shortcut(&k, &sc.name) {
        // 氏名の入力ボックスを開く。
        app.input_box = Some(InputBoxState {
            prompt: "Full name:".into(),
            value: app.full_name.clone(),
            cursor: app.full_name.chars().count(),
            callback_id: InputCallbackId::SettingsFullName,
        });
    }

    Ok(false)
}

/// 編集画面のキー処理。
async fn handle_edit_job_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // 編集画面のショートカットを参照する。
    let sc = &app.shortcuts.edit_job;

    if shortcuts::matches_shortcut(&k, &sc.cancel) {
        // 編集をやめてメイン画面へ戻る。
        app.ui.screen = Screen::Main;
    } else if shortcuts::matches_shortcut(&k, &sc.next_field) {
        // 次の編集フィールドへ移動する。
        app.ui.editing_field_idx = (app.ui.editing_field_idx + 1) % 5;
    } else if shortcuts::matches_shortcut(&k, &sc.commit) {
        // 選択ジョブを確定してWorkerへ送る。
        let Some(job) = app.jobs.get(app.ui.selected).cloned() else {
            return Ok(false);
        };
        // 編集内容と対象月を送信する。
        app.worker_tx
            .send(WorkerCmd::CommitJobEdits {
                job_id: job.id,
                fields: job.fields,
                target_month_ym: app.edit_target_month.clone(),
            })
            .await?;
        // 画面を戻して進行状況を表示する。
        app.ui.screen = Screen::Main;
        app.ui.status = "Committed (writing sheet/exporting pdf...)".into();
    } else if shortcuts::matches_shortcut(&k, &sc.target_month) {
        // 対象月の入力ボックスを開く。
        app.input_box = Some(InputBoxState {
            prompt: "Target month (YYYY-MM):".into(),
            value: app.edit_target_month.clone(),
            cursor: app.edit_target_month.chars().count(),
            callback_id: InputCallbackId::EditTargetMonth,
        });
    } else if shortcuts::matches_shortcut(&k, &sc.edit_field)
        && let Some(j) = app.jobs.get(app.ui.selected)
    {
        // 現在の編集対象フィールドに応じて入力ボックスを用意する。
        let (prompt, value, field_idx) = match app.ui.editing_field_idx {
            0 => ("Date (YYYY-MM-DD):", j.fields.date_ymd.clone(), 0),
            1 => ("Reason:", j.fields.reason.clone(), 1),
            2 => ("Amount (yen):", j.fields.amount_yen.to_string(), 2),
            3 => ("Category:", j.fields.category.clone(), 3),
            4 => ("Note:", j.fields.note.clone(), 4),
            _ => return Ok(false),
        };
        // 入力ボックスを表示する。
        app.input_box = Some(InputBoxState {
            prompt: prompt.into(),
            value,
            cursor: 0,
            callback_id: InputCallbackId::EditJobField(field_idx),
        });
    }

    Ok(false)
}

/// 初期設定ウィザード画面のキー処理。
async fn handle_wizard_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // ウィザード画面のショートカットを参照する。
    let sc = &app.shortcuts.wizard;

    if shortcuts::matches_shortcut(&k, &sc.proceed) {
        use wizard::WizardStep;
        match &app.wizard_state.current_step {
            WizardStep::Welcome => {
                // 次のステップへ進む。
                app.wizard_state.next_step();
            }
            WizardStep::CheckAuth => {
                // credentials.json の存在チェックを行う。
                if !std::path::Path::new("assets/credentials.json").exists() {
                    app.ui.error = Some("assets/credentials.json not found. Please add it.".into());
                } else {
                    // エラーを解除して次へ進む。
                    app.ui.error = None;
                    app.wizard_state.next_step();
                }
            }
            WizardStep::InputFolderId => {
                // 入力フォルダID入力を促す。
                app.input_box = Some(InputBoxState {
                    prompt: "Input folder ID:".into(),
                    value: app.in_folder.clone(),
                    cursor: app.in_folder.chars().count(),
                    callback_id: InputCallbackId::WizardInputFolder,
                });
            }
            WizardStep::OutputFolderId => {
                // 出力フォルダID入力を促す。
                app.input_box = Some(InputBoxState {
                    prompt: "Output folder ID:".into(),
                    value: app.out_folder.clone(),
                    cursor: app.out_folder.chars().count(),
                    callback_id: InputCallbackId::WizardOutputFolder,
                });
            }
            WizardStep::TemplateSheetId => {
                // テンプレートシートID入力を促す。
                app.input_box = Some(InputBoxState {
                    prompt: "Template sheet ID:".into(),
                    value: app.template_id.clone(),
                    cursor: app.template_id.chars().count(),
                    callback_id: InputCallbackId::WizardTemplateId,
                });
            }
            WizardStep::UserName => {
                // 氏名入力を促す。
                app.input_box = Some(InputBoxState {
                    prompt: "Your full name:".into(),
                    value: app.full_name.clone(),
                    cursor: app.full_name.chars().count(),
                    callback_id: InputCallbackId::WizardFullName,
                });
            }
            WizardStep::Complete => {
                // 必須項目が揃っているか検証する。
                if app.in_folder.is_empty()
                    || app.out_folder.is_empty()
                    || app.template_id.is_empty()
                {
                    app.ui.error = Some("Required fields are missing.".into());
                    app.wizard_state.current_step = WizardStep::InputFolderId;
                    return Ok(false);
                }

                // 設定を保存する。
                app.cfg.google.input_folder_id = app.in_folder.clone();
                app.cfg.google.output_folder_id = app.out_folder.clone();
                app.cfg.google.template_sheet_id = app.template_id.clone();
                app.cfg.user.full_name = app.full_name.clone();
                app.cfg.save(&app.cfg_path)?;

                // Workerへ設定更新を通知する。
                app.worker_tx
                    .send(WorkerCmd::SaveSettings(app.cfg.clone()))
                    .await?;

                // メイン画面へ移動して一覧を更新する。
                app.ui.screen = Screen::Main;
                app.ui.status = "Setup complete!".into();
                request_refresh(app).await?;
            }
        }
    } else if shortcuts::matches_shortcut(&k, &sc.skip) {
        // 現在のステップをスキップする。
        app.wizard_state.next_step();
    }

    Ok(false)
}

/// 入力ボックスのキー処理。
async fn handle_input_box_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // 入力ボックスが無ければ何もしない。
    let Some(input_state) = &mut app.input_box else {
        return Ok(false);
    };

    // 入力ボックス用ショートカットを参照する。
    let sc = &app.shortcuts.input_box;

    // 入力ボックス中でもCtrl+Cで終了できるようにする。
    if k.modifiers
        .contains(crossterm::event::KeyModifiers::CONTROL)
        && k.code == KeyCode::Char('c')
    {
        return Ok(true);
    }

    if shortcuts::matches_shortcut(&k, &sc.confirm) {
        // 入力ボックスを閉じる前に値とコールバック種別を保存する。
        let value = input_state.value.clone();
        let callback_id = input_state.callback_id.clone();
        app.input_box = None;

        // コールバック種別に応じて値を反映する。
        match callback_id {
            InputCallbackId::SettingsInputFolder => app.in_folder = value,
            InputCallbackId::SettingsOutputFolder => app.out_folder = value,
            InputCallbackId::SettingsTemplateId => app.template_id = value,
            InputCallbackId::SettingsFullName => app.full_name = value,
            InputCallbackId::EditTargetMonth => app.edit_target_month = value,
            InputCallbackId::EditJobField(field_idx) => {
                // 対象ジョブのフィールドを更新する。
                if let Some(j) = app.jobs.get_mut(app.ui.selected) {
                    match field_idx {
                        0 => j.fields.date_ymd = value,
                        1 => j.fields.reason = value,
                        2 => j.fields.amount_yen = value.parse().unwrap_or(0),
                        3 => j.fields.category = value,
                        4 => j.fields.note = value,
                        _ => {}
                    }
                }
            }
            InputCallbackId::WizardInputFolder => {
                // ウィザードの入力フォルダIDを更新し次へ進む。
                app.in_folder = value;
                app.wizard_state.next_step();
            }
            InputCallbackId::WizardOutputFolder => {
                // ウィザードの出力フォルダIDを更新し次へ進む。
                app.out_folder = value;
                app.wizard_state.next_step();
            }
            InputCallbackId::WizardTemplateId => {
                // ウィザードのテンプレートIDを更新し次へ進む。
                app.template_id = value;
                app.wizard_state.next_step();
            }
            InputCallbackId::WizardFullName => {
                // ウィザードの氏名を更新し次へ進む。
                app.full_name = value;
                app.wizard_state.next_step();
            }
        }
    } else if shortcuts::matches_shortcut(&k, &sc.cancel) {
        // 入力を破棄して入力ボックスを閉じる。
        app.input_box = None;
    } else if shortcuts::matches_shortcut(&k, &sc.backspace) {
        // バックスペースを処理する。
        input_state.backspace();
    } else if shortcuts::matches_shortcut(&k, &sc.delete) {
        // デリートを処理する。
        input_state.delete();
    } else if shortcuts::matches_shortcut(&k, &sc.left) {
        // 左移動を処理する。
        input_state.move_left();
    } else if shortcuts::matches_shortcut(&k, &sc.right) {
        // 右移動を処理する。
        input_state.move_right();
    } else if shortcuts::matches_shortcut(&k, &sc.home) {
        // 行頭移動を処理する。
        input_state.move_home();
    } else if shortcuts::matches_shortcut(&k, &sc.end) {
        // 行末移動を処理する。
        input_state.move_end();
    } else if shortcuts::matches_shortcut(&k, &sc.clear_line) {
        // 行をクリアする。
        input_state.clear_line();
    } else if let KeyCode::Char(c) = k.code {
        // 通常の文字入力を処理する。
        if !k
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            // コントロールキーでない場合のみ挿入する。
            input_state.insert_char(c);
        }
    }

    Ok(false)
}

/// 設定画面用の編集バッファを設定値から再読み込みする。
fn reload_settings_buffers(app: &mut App) {
    // 設定の現在値を編集用バッファへ反映する。
    app.in_folder = app.cfg.google.input_folder_id.clone();
    app.out_folder = app.cfg.google.output_folder_id.clone();
    app.template_id = app.cfg.google.template_sheet_id.clone();
    app.full_name = app.cfg.user.full_name.clone();
}

/// ジョブ状態を一覧表示用の短いラベルへ変換する。
fn status_str(s: &JobStatus) -> String {
    match s {
        JobStatus::Queued => "Queued".into(),
        JobStatus::WaitingUserFix => "Edit".into(),
        JobStatus::WritingSheet => "WriteSheet".into(),
        JobStatus::ExportingPdf => "ExportPdf".into(),
        JobStatus::UploadingPdf => "UploadPdf".into(),
        JobStatus::Done => "Done".into(),
        JobStatus::Error(e) => format!("Error: {e}"),
    }
}

/// 画面全体のレイアウトを描画する。
fn draw(f: &mut Frame, app: &App) {
    // ウィザード画面は専用描画で処理する。
    if app.ui.screen == Screen::InitialSetup {
        draw_wizard_screen(f, app);
        // 入力ボックスが開いていれば重ねて描画する。
        if let Some(input_state) = &app.input_box {
            input::render_input_box(f, input_state);
        }
        return;
    }

    // メインレイアウト（Body + HELP + STATUS）を作る。
    let main_layout = layout::create_main_layout(f.area());
    let body_layout = layout::create_body_layout(main_layout.body);

    // ジョブ一覧からテーブル行を組み立てる。
    let rows = app.jobs.iter().enumerate().map(|(i, j)| {
        Row::new(vec![
            format!("{}", i + 1),
            j.filename.clone(),
            status_str(&j.status),
            j.fields.amount_yen.to_string(),
            j.fields.date_ymd.clone(),
        ])
    });

    // ジョブテーブルのウィジェットを構築する。
    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(12),
        ],
    )
    .block(Block::default().borders(Borders::ALL).title("JOBS"))
    .header(Row::new(vec!["#", "file", "status", "amount", "date"]).bold())
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(255, 140, 0)) // オレンジ色の背景
            .fg(Color::Black) // 黒文字
            .add_modifier(Modifier::BOLD),
    );

    // 選択中の行をハイライトする。
    let mut table_state = ratatui::widgets::TableState::default();
    if !app.jobs.is_empty() {
        table_state.select(Some(app.ui.selected));
    }
    // テーブルを描画する。
    f.render_stateful_widget(table, body_layout.jobs_table, &mut table_state);

    // 選択中のファイル情報（またはプレースホルダ）を用意する。
    let (sel_name, sel_id) = if let Some(j) = app.jobs.get(app.ui.selected) {
        (j.filename.clone(), j.drive_file_id.clone())
    } else {
        ("-".into(), "-".into())
    };

    // 右パネル：通常は選択情報/設定/ログ、編集画面ではフィールド強調表示。
    let info_text = if app.ui.screen == Screen::EditJob {
        if let Some(job) = app.jobs.get(app.ui.selected) {
            // 編集対象フィールド一覧を作成する。
            let fields = [
                ("Date", &job.fields.date_ymd),
                ("Reason", &job.fields.reason),
                ("Amount", &job.fields.amount_yen.to_string()),
                ("Category", &job.fields.category),
                ("Note", &job.fields.note),
            ];
            let mut lines = vec![
                format!("Editing: {}", job.filename),
                String::new(),
                "Fields (use Tab to navigate):".to_string(),
            ];
            // 現在選択中のフィールドに印を付ける。
            for (i, (name, value)) in fields.iter().enumerate() {
                let marker = if i == app.ui.editing_field_idx {
                    "→" // 現在のフィールドを矢印で示す
                } else {
                    " "
                };
                lines.push(format!("{} [{}] {}: {}", marker, i, name, value));
            }
            // 対象月の情報も追加する。
            lines.push(String::new());
            lines.push(format!("Target Month: {}", app.edit_target_month));
            lines.join("\n")
        } else {
            "No job selected".to_string()
        }
    } else {
        // 通常画面用の情報文字列を組み立てる。
        format!(
            "Selected: {}\nSelected ID: {}\n\nIn: {}\nOut: {}\nTpl: {}\nName: {}\nMonth: {}\n\nLog:\n{}",
            sel_name,
            sel_id,
            app.cfg.google.input_folder_id,
            app.cfg.google.output_folder_id,
            app.cfg.google.template_sheet_id,
            app.cfg.user.full_name,
            app.edit_target_month,
            app.ui
                .log
                .iter()
                .rev()
                .take(8)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };
    // INFOパネルとして描画する。
    let info_panel = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL).title("INFO"))
        .wrap(Wrap { trim: true });
    f.render_widget(info_panel, body_layout.info_panel);

    // HELPバー（画面ごとのショートカット）を描画する。
    let help_text = get_help_text(&app.ui.screen, &app.shortcuts);
    let help_bar = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("HELP"))
        .wrap(Wrap { trim: true });
    f.render_widget(help_bar, main_layout.help_bar);

    // STATUSバー（画面名・ジョブ情報・エラー）を描画する。
    let screen_name = match app.ui.screen {
        Screen::Main => "Main",
        Screen::Settings => "Settings",
        Screen::EditJob => "EditJob",
        Screen::InitialSetup => "Setup",
    };

    // ジョブ件数と完了数を集計する。
    let job_info = format!(
        "Jobs: {} total, {} done",
        app.jobs.len(),
        app.jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Done))
            .count()
    );

    // エラーの有無でステータス文字列を切り替える。
    let status_text = if let Some(err) = &app.ui.error {
        format!("[{}] {} | ERROR: {}", screen_name, job_info, err)
    } else {
        format!("[{}] {} | {}", screen_name, job_info, app.ui.status)
    };

    // ステータスバーのウィジェットを生成する。
    let mut status_bar = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL).title("STATUS"))
        .wrap(Wrap { trim: true });

    // エラー時は赤色で強調表示する。
    if app.ui.error.is_some() {
        status_bar = status_bar.style(Style::default().fg(Color::Red));
    }

    // ステータスバーを描画する。
    f.render_widget(status_bar, main_layout.status_bar);

    // 入力ボックスが開いていれば重ねて描画する。
    if let Some(input_state) = &app.input_box {
        input::render_input_box(f, input_state);
    }
}

/// ウィザード画面を描画する。
fn draw_wizard_screen(f: &mut Frame, app: &App) {
    // 余白込みで縦方向に3分割する。
    let outer_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20), // 上部マージン
            Constraint::Min(10),        // 本文領域
            Constraint::Percentage(20), // 下部マージン
        ])
        .split(f.area());

    // ステップ番号と総数、プロンプトを取得する。
    let step_num = app.wizard_state.get_step_number();
    let total_steps = app.wizard_state.total_steps;
    let prompt = app.wizard_state.get_prompt();

    // 表示するテキストを組み立てる。
    let content_text = format!(
        "=== Initial Setup Wizard ===\n\nStep {}/{}\n\n{}\n\nPress Enter to proceed, ESC to skip step.",
        step_num, total_steps, prompt
    );

    // メインの本文を描画する。
    let content = Paragraph::new(content_text)
        .block(Block::default().borders(Borders::ALL).title("Setup"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(content, outer_layout[1]);

    // エラーがあれば下部に表示する。
    if let Some(err) = &app.ui.error {
        // エラー表示用のレイアウトを作成する。
        let error_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.area());

        // エラー用のパネルを構成する。
        let error_text = Paragraph::new(format!("ERROR: {}", err))
            .block(Block::default().borders(Borders::ALL).title("Error"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });

        // エラー表示を描画する。
        f.render_widget(error_text, error_layout[1]);
    }
}

/// 現在画面に応じたヘルプ文字列を返す。
fn get_help_text(screen: &Screen, shortcuts: &Shortcuts) -> String {
    match screen {
        Screen::Main => format!(
            "{}: quit | {}: refresh | {}: settings | {}: edit | {}↑{}: navigate",
            shortcuts.main.quit,
            shortcuts.main.refresh,
            shortcuts.main.settings,
            shortcuts.main.enter,
            shortcuts.main.up,
            shortcuts.main.down
        ),
        Screen::Settings => format!(
            "{}: input folder | {}: output folder | {}: template | {}: name | {}: save | {}: cancel",
            shortcuts.settings.input_folder,
            shortcuts.settings.output_folder,
            shortcuts.settings.template,
            shortcuts.settings.name,
            shortcuts.settings.save,
            shortcuts.settings.cancel
        ),
        Screen::EditJob => format!(
            "{}: edit field | {}: next field | {}: month | {}: commit | {}: cancel",
            shortcuts.edit_job.edit_field,
            shortcuts.edit_job.next_field,
            shortcuts.edit_job.target_month,
            shortcuts.edit_job.commit,
            shortcuts.edit_job.cancel
        ),
        Screen::InitialSetup => format!(
            "Follow wizard steps | {}: proceed | {}: skip step",
            shortcuts.wizard.proceed, shortcuts.wizard.skip
        ),
    }
}
