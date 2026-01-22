//! TUI描画関連の関数。

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
};

use crate::{events::Screen, input, jobs::JobStatus, layout, shortcuts::Shortcuts};

use super::App;

/// 画面全体のレイアウトを描画する。
pub fn draw(f: &mut Frame, app: &App) {
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
        build_edit_info_text(app)
    } else {
        build_main_info_text(app, &sel_name, &sel_id)
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
    let status_bar = build_status_bar(app);
    f.render_widget(status_bar, main_layout.status_bar);

    // 入力ボックスが開いていれば重ねて描画する。
    if let Some(input_state) = &app.input_box {
        input::render_input_box(f, input_state);
    }
}

/// 編集画面用の情報テキストを構築する。
fn build_edit_info_text(app: &App) -> String {
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
}

/// メイン画面用の情報テキストを構築する。
fn build_main_info_text(app: &App, sel_name: &str, sel_id: &str) -> String {
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
}

/// ステータスバーを構築する。
fn build_status_bar(app: &App) -> Paragraph<'static> {
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

    status_bar
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
            "{}: quit | {}: refresh | {}: settings | {}: edit | {}/{}↑: navigate",
            format_keys(&shortcuts.main.quit),
            format_keys(&shortcuts.main.refresh),
            format_keys(&shortcuts.main.settings),
            format_keys(&shortcuts.main.enter),
            format_keys(&shortcuts.main.up),
            format_keys(&shortcuts.main.down)
        ),
        Screen::Settings => format!(
            "{}: input folder | {}: output folder | {}: template | {}: name | {}: save | {}: cancel",
            format_keys(&shortcuts.settings.input_folder),
            format_keys(&shortcuts.settings.output_folder),
            format_keys(&shortcuts.settings.template),
            format_keys(&shortcuts.settings.name),
            format_keys(&shortcuts.settings.save),
            format_keys(&shortcuts.settings.cancel)
        ),
        Screen::EditJob => format!(
            "{}: edit field | {}: next field | {}: month | {}: commit | {}: cancel",
            format_keys(&shortcuts.edit_job.edit_field),
            format_keys(&shortcuts.edit_job.next_field),
            format_keys(&shortcuts.edit_job.target_month),
            format_keys(&shortcuts.edit_job.commit),
            format_keys(&shortcuts.edit_job.cancel)
        ),
        Screen::InitialSetup => format!(
            "Follow wizard steps | {}: proceed | {}: skip step",
            format_keys(&shortcuts.wizard.proceed),
            format_keys(&shortcuts.wizard.skip)
        ),
    }
}

/// ショートカットキーの配列を表示用文字列に変換する。
fn format_keys(keys: &[String]) -> String {
    keys.join("/")
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
