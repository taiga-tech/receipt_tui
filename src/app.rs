//! TUI event loop, input handling, and rendering.

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
    ui::Tui,
    wizard,
    worker::{self, WorkerCmd, WorkerEvent},
};

/// In-memory application state shared between input handling and rendering.
pub struct App {
    /// Path to the persisted config file.
    pub cfg_path: PathBuf,
    /// Current config values in memory.
    pub cfg: Config,
    /// UI-specific state such as selection and status text.
    pub ui: UiState,
    /// Jobs loaded from Drive (one per image file).
    pub jobs: Vec<Job>,
    /// Command channel to the background worker.
    pub worker_tx: mpsc::Sender<WorkerCmd>,
    /// Event channel from the background worker.
    pub worker_rx: mpsc::Receiver<WorkerEvent>,

    /// Editable input folder id for the settings screen.
    pub in_folder: String,
    /// Editable output folder id for the settings screen.
    pub out_folder: String,
    /// Editable template sheet id for the settings screen.
    pub template_id: String,
    /// Editable user name for the settings screen.
    pub full_name: String,

    /// Target month for inserting a receipt row (YYYY-MM).
    pub edit_target_month: String,

    /// Current InputBox state (Some when input is active).
    pub input_box: Option<InputBoxState>,

    /// Wizard state for initial setup.
    pub wizard_state: wizard::WizardState,
}

/// Run the main TUI loop until the user exits.
pub async fn run_app(terminal: &mut Tui) -> Result<()> {
    // Load config or create a default file on first run.
    let cfg_path = PathBuf::from("config.toml");
    let cfg = Config::load_or_default(&cfg_path)?;

    // Dedicated channels for worker commands and events.
    let (tx_cmd, rx_cmd) = mpsc::channel::<WorkerCmd>(64);
    let (tx_ev, rx_ev) = mpsc::channel::<WorkerEvent>(256);

    // Spawn the background worker with an initial config snapshot.
    tokio::spawn(worker::run(rx_cmd, tx_ev, cfg.clone()));

    // Determine initial screen based on config completeness.
    let initial_screen = if needs_initial_setup(&cfg) {
        Screen::InitialSetup
    } else {
        Screen::Main
    };

    // Auto-generate edit_target_month from current date.
    let now = chrono::Local::now();
    let edit_target_month = format!("{}-{:02}", now.year(), now.month());

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
    };

    // Initial refresh only if not in wizard mode.
    if initial_screen == Screen::Main {
        request_refresh(&mut app).await?;
    }

    loop {
        // Render the current state to the terminal.
        terminal.draw(|f| draw(f, &app))?;

        // Drain worker events before handling the next input.
        while let Ok(ev) = app.worker_rx.try_recv() {
            handle_worker_event(&mut app, ev)?;
        }

        // Poll for input with a small timeout to keep the UI responsive.
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(k) = event::read()?
            && handle_key(&mut app, k).await?
        {
            break;
        }
    }
    Ok(())
}

/// Apply a worker event to the UI state.
fn handle_worker_event(app: &mut App, ev: WorkerEvent) -> Result<()> {
    match ev {
        WorkerEvent::AuthSuccess => {
            app.ui.error = None;
            app.ui.status = "Authentication successful".into();
            // If in wizard CheckAuth step, move to next step
            if app.ui.screen == Screen::InitialSetup
                && app.wizard_state.current_step == wizard::WizardStep::CheckAuth
            {
                app.wizard_state.next_step();
            }
        }
        WorkerEvent::AuthFailed(e) => {
            app.ui.error = Some(format!("Authentication failed: {}", e));
            app.ui.status = format!("Auth failed: {e}");
        }
        WorkerEvent::JobsLoaded(jobs) => {
            app.jobs = jobs;
            app.ui.selected = 0;
            app.ui.status = format!("Loaded {} jobs", app.jobs.len());
        }
        WorkerEvent::JobUpdated { job_id, status } => {
            if let Some(j) = app.jobs.iter_mut().find(|j| j.id == job_id) {
                j.status = status;
            }
        }
        WorkerEvent::Log(s) => app.ui.log.push(s),
        WorkerEvent::Error(s) => app.ui.status = format!("Error: {s}"),
    }
    Ok(())
}

/// Ask the worker to refresh jobs if required settings exist.
async fn request_refresh(app: &mut App) -> Result<()> {
    if app.cfg.google.input_folder_id.is_empty()
        || app.cfg.google.output_folder_id.is_empty()
        || app.cfg.google.template_sheet_id.is_empty()
    {
        app.ui.status = "Settings required (press t)".into();
        tracing::warn!("refresh skipped: settings required");
    } else {
        tracing::info!("refresh requested");
        app.worker_tx.send(WorkerCmd::RefreshJobs).await?;
        app.ui.status = "Refreshing jobs...".into();
    }
    Ok(())
}

/// Check if initial setup wizard is needed.
fn needs_initial_setup(cfg: &Config) -> bool {
    cfg.google.input_folder_id.is_empty()
        || cfg.google.output_folder_id.is_empty()
        || cfg.google.template_sheet_id.is_empty()
        || cfg.user.full_name == "Your Name"
}

/// Handle a single key press; returns true when the app should exit.
async fn handle_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // InputBox has priority - if active, handle keys for input box
    if app.input_box.is_some() {
        return handle_input_box_key(app, k).await;
    }

    // Delegate to screen-specific handlers
    match app.ui.screen {
        Screen::Main => handle_main_key(app, k).await,
        Screen::Settings => handle_settings_key(app, k).await,
        Screen::EditJob => handle_edit_job_key(app, k).await,
        Screen::InitialSetup => handle_wizard_key(app, k).await,
    }
}

/// Handle keys for Main screen
async fn handle_main_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    match k.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('t') => {
            reload_settings_buffers(app);
            app.ui.screen = Screen::Settings;
            app.ui.status = "Settings".into();
        }
        KeyCode::Char('r') => {
            request_refresh(app).await?;
        }
        KeyCode::Down => {
            if app.ui.selected + 1 < app.jobs.len() {
                app.ui.selected += 1;
            }
        }
        KeyCode::Up => {
            if app.ui.selected > 0 {
                app.ui.selected -= 1;
            }
        }
        KeyCode::Enter => {
            if app.jobs.get(app.ui.selected).is_some() {
                app.ui.screen = Screen::EditJob;
                app.ui.editing_field_idx = 0;
            }
        }
        _ => {}
    }
    Ok(false)
}

/// Handle keys for Settings screen
async fn handle_settings_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    match k.code {
        KeyCode::Esc => {
            reload_settings_buffers(app);
            app.ui.screen = Screen::Main;
        }
        KeyCode::Enter => {
            app.cfg.google.input_folder_id = app.in_folder.clone();
            app.cfg.google.output_folder_id = app.out_folder.clone();
            app.cfg.google.template_sheet_id = app.template_id.clone();
            app.cfg.user.full_name = app.full_name.clone();
            app.cfg.save(&app.cfg_path)?;

            app.worker_tx
                .send(WorkerCmd::SaveSettings(app.cfg.clone()))
                .await?;
            app.ui.screen = Screen::Main;
            app.ui.status = "Saved settings".into();
        }
        KeyCode::Char('i') => {
            app.input_box = Some(InputBoxState {
                prompt: "Input folder ID:".into(),
                value: app.in_folder.clone(),
                cursor: app.in_folder.chars().count(),
                callback_id: InputCallbackId::SettingsInputFolder,
            });
        }
        KeyCode::Char('o') => {
            app.input_box = Some(InputBoxState {
                prompt: "Output folder ID:".into(),
                value: app.out_folder.clone(),
                cursor: app.out_folder.chars().count(),
                callback_id: InputCallbackId::SettingsOutputFolder,
            });
        }
        KeyCode::Char('p') => {
            app.input_box = Some(InputBoxState {
                prompt: "Template sheet ID:".into(),
                value: app.template_id.clone(),
                cursor: app.template_id.chars().count(),
                callback_id: InputCallbackId::SettingsTemplateId,
            });
        }
        KeyCode::Char('n') => {
            app.input_box = Some(InputBoxState {
                prompt: "Full name:".into(),
                value: app.full_name.clone(),
                cursor: app.full_name.chars().count(),
                callback_id: InputCallbackId::SettingsFullName,
            });
        }
        _ => {}
    }
    Ok(false)
}

/// Handle keys for EditJob screen
async fn handle_edit_job_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    match k.code {
        KeyCode::Esc => app.ui.screen = Screen::Main,
        KeyCode::Tab => app.ui.editing_field_idx = (app.ui.editing_field_idx + 1) % 5,
        KeyCode::Enter => {
            let Some(job) = app.jobs.get(app.ui.selected).cloned() else {
                return Ok(false);
            };
            app.worker_tx
                .send(WorkerCmd::CommitJobEdits {
                    job_id: job.id,
                    fields: job.fields,
                    target_month_ym: app.edit_target_month.clone(),
                })
                .await?;
            app.ui.screen = Screen::Main;
            app.ui.status = "Committed (writing sheet/exporting pdf...)".into();
        }
        KeyCode::Char('m') => {
            app.input_box = Some(InputBoxState {
                prompt: "Target month (YYYY-MM):".into(),
                value: app.edit_target_month.clone(),
                cursor: app.edit_target_month.chars().count(),
                callback_id: InputCallbackId::EditTargetMonth,
            });
        }
        KeyCode::Char('e') => {
            if let Some(j) = app.jobs.get(app.ui.selected) {
                let (prompt, value, field_idx) = match app.ui.editing_field_idx {
                    0 => ("Date (YYYY-MM-DD):", j.fields.date_ymd.clone(), 0),
                    1 => ("Reason:", j.fields.reason.clone(), 1),
                    2 => ("Amount (yen):", j.fields.amount_yen.to_string(), 2),
                    3 => ("Category:", j.fields.category.clone(), 3),
                    4 => ("Note:", j.fields.note.clone(), 4),
                    _ => return Ok(false),
                };
                app.input_box = Some(InputBoxState {
                    prompt: prompt.into(),
                    value,
                    cursor: 0,
                    callback_id: InputCallbackId::EditJobField(field_idx),
                });
            }
        }
        _ => {}
    }
    Ok(false)
}

/// Handle keys for InitialSetup (wizard) screen
async fn handle_wizard_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    // Allow Ctrl+C to quit
    if k.modifiers
        .contains(crossterm::event::KeyModifiers::CONTROL)
        && k.code == KeyCode::Char('c')
    {
        return Ok(true);
    }

    match k.code {
        KeyCode::Esc => {
            // Allow ESC to exit wizard
            return Ok(true);
        }
        KeyCode::Enter => {
            use wizard::WizardStep;
            match &app.wizard_state.current_step {
                WizardStep::Welcome => {
                    app.wizard_state.next_step();
                }
                WizardStep::CheckAuth => {
                    if !std::path::Path::new("assets/credentials.json").exists() {
                        app.ui.error =
                            Some("assets/credentials.json not found. Please add it.".into());
                    } else {
                        app.ui.error = None;
                        app.ui.status = "Authenticating... (please check your browser)".into();
                        // Send CheckAuth command to worker
                        app.worker_tx.send(WorkerCmd::CheckAuth).await?;
                    }
                }
                WizardStep::InputFolderId => {
                    app.input_box = Some(InputBoxState {
                        prompt: "Input folder ID:".into(),
                        value: app.in_folder.clone(),
                        cursor: app.in_folder.chars().count(),
                        callback_id: InputCallbackId::WizardInputFolder,
                    });
                }
                WizardStep::OutputFolderId => {
                    app.input_box = Some(InputBoxState {
                        prompt: "Output folder ID:".into(),
                        value: app.out_folder.clone(),
                        cursor: app.out_folder.chars().count(),
                        callback_id: InputCallbackId::WizardOutputFolder,
                    });
                }
                WizardStep::TemplateSheetId => {
                    app.input_box = Some(InputBoxState {
                        prompt: "Template sheet ID:".into(),
                        value: app.template_id.clone(),
                        cursor: app.template_id.chars().count(),
                        callback_id: InputCallbackId::WizardTemplateId,
                    });
                }
                WizardStep::UserName => {
                    app.input_box = Some(InputBoxState {
                        prompt: "Your full name:".into(),
                        value: app.full_name.clone(),
                        cursor: app.full_name.chars().count(),
                        callback_id: InputCallbackId::WizardFullName,
                    });
                }
                WizardStep::Complete => {
                    // Validate required fields
                    if app.in_folder.is_empty()
                        || app.out_folder.is_empty()
                        || app.template_id.is_empty()
                    {
                        app.ui.error = Some("Required fields are missing.".into());
                        app.wizard_state.current_step = WizardStep::InputFolderId;
                        return Ok(false);
                    }

                    // Save settings
                    app.cfg.google.input_folder_id = app.in_folder.clone();
                    app.cfg.google.output_folder_id = app.out_folder.clone();
                    app.cfg.google.template_sheet_id = app.template_id.clone();
                    app.cfg.user.full_name = app.full_name.clone();
                    app.cfg.save(&app.cfg_path)?;

                    app.worker_tx
                        .send(WorkerCmd::SaveSettings(app.cfg.clone()))
                        .await?;

                    app.ui.screen = Screen::Main;
                    app.ui.status = "Setup complete!".into();
                    request_refresh(app).await?;
                }
            }
        }
        _ => {}
    }
    Ok(false)
}

/// Handle keys for InputBox
async fn handle_input_box_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    let Some(input_state) = &mut app.input_box else {
        return Ok(false);
    };

    // Allow Ctrl+C to quit even in InputBox
    if k.modifiers
        .contains(crossterm::event::KeyModifiers::CONTROL)
        && k.code == KeyCode::Char('c')
    {
        return Ok(true);
    }

    match k.code {
        KeyCode::Enter => {
            // Get the value and callback_id before closing the input box
            let value = input_state.value.clone();
            let callback_id = input_state.callback_id.clone();
            app.input_box = None;

            // Apply the value based on callback_id
            match callback_id {
                InputCallbackId::SettingsInputFolder => app.in_folder = value,
                InputCallbackId::SettingsOutputFolder => app.out_folder = value,
                InputCallbackId::SettingsTemplateId => app.template_id = value,
                InputCallbackId::SettingsFullName => app.full_name = value,
                InputCallbackId::EditTargetMonth => app.edit_target_month = value,
                InputCallbackId::EditJobField(field_idx) => {
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
                    app.in_folder = value;
                    app.wizard_state.next_step();
                }
                InputCallbackId::WizardOutputFolder => {
                    app.out_folder = value;
                    app.wizard_state.next_step();
                }
                InputCallbackId::WizardTemplateId => {
                    app.template_id = value;
                    app.wizard_state.next_step();
                }
                InputCallbackId::WizardFullName => {
                    app.full_name = value;
                    app.wizard_state.next_step();
                }
            }
        }
        KeyCode::Esc => {
            app.input_box = None;
        }
        KeyCode::Backspace => {
            input_state.backspace();
        }
        KeyCode::Delete => {
            input_state.delete();
        }
        KeyCode::Left => {
            input_state.move_left();
        }
        KeyCode::Right => {
            input_state.move_right();
        }
        KeyCode::Home => {
            input_state.move_home();
        }
        KeyCode::End => {
            input_state.move_end();
        }
        KeyCode::Char(c) => {
            if k.modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            {
                if c == 'u' {
                    input_state.clear_line();
                }
            } else {
                input_state.insert_char(c);
            }
        }
        _ => {}
    }

    Ok(false)
}

/// Reload settings buffers from config (for Settings screen entry or ESC)
fn reload_settings_buffers(app: &mut App) {
    app.in_folder = app.cfg.google.input_folder_id.clone();
    app.out_folder = app.cfg.google.output_folder_id.clone();
    app.template_id = app.cfg.google.template_sheet_id.clone();
    app.full_name = app.cfg.user.full_name.clone();
}

/// Convert a job status into a short label for the table.
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

/// Render the full UI layout.
fn draw(f: &mut Frame, app: &App) {
    // Handle wizard screen separately
    if app.ui.screen == Screen::InitialSetup {
        draw_wizard_screen(f, app);
        // Render InputBox if active
        if let Some(input_state) = &app.input_box {
            input::render_input_box(f, input_state);
        }
        return;
    }

    // Main layout: Body + HELP + STATUS
    let main_layout = layout::create_main_layout(f.area());
    let body_layout = layout::create_body_layout(main_layout.body);

    // Build rows from the current jobs list.
    let rows = app.jobs.iter().enumerate().map(|(i, j)| {
        Row::new(vec![
            format!("{}", i + 1),
            j.filename.clone(),
            status_str(&j.status),
            j.fields.amount_yen.to_string(),
            j.fields.date_ymd.clone(),
        ])
    });

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

    // Keep the highlight on the current selection.
    let mut table_state = ratatui::widgets::TableState::default();
    if !app.jobs.is_empty() {
        table_state.select(Some(app.ui.selected));
    }
    f.render_stateful_widget(table, body_layout.jobs_table, &mut table_state);

    // Show selected file info or placeholders when empty.
    let (sel_name, sel_id) = if let Some(j) = app.jobs.get(app.ui.selected) {
        (j.filename.clone(), j.drive_file_id.clone())
    } else {
        ("-".into(), "-".into())
    };

    // Right panel shows selection info, settings, and recent logs.
    // In EditJob screen, show field highlights
    let info_text = if app.ui.screen == Screen::EditJob {
        if let Some(job) = app.jobs.get(app.ui.selected) {
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
            for (i, (name, value)) in fields.iter().enumerate() {
                let marker = if i == app.ui.editing_field_idx {
                    "→" // 現在のフィールドを矢印で示す
                } else {
                    " "
                };
                lines.push(format!("{} [{}] {}: {}", marker, i, name, value));
            }
            lines.push(String::new());
            lines.push(format!("Target Month: {}", app.edit_target_month));
            lines.join("\n")
        } else {
            "No job selected".to_string()
        }
    } else {
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
    let info_panel = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL).title("INFO"))
        .wrap(Wrap { trim: true });
    f.render_widget(info_panel, body_layout.info_panel);

    // HELP bar with context-sensitive keyboard shortcuts
    let help_text = get_help_text(&app.ui.screen);
    let help_bar = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("HELP"))
        .wrap(Wrap { trim: true });
    f.render_widget(help_bar, main_layout.help_bar);

    // STATUS bar with screen name, job info, and error highlighting
    let screen_name = match app.ui.screen {
        Screen::Main => "Main",
        Screen::Settings => "Settings",
        Screen::EditJob => "EditJob",
        Screen::InitialSetup => "Setup",
    };

    let job_info = format!(
        "Jobs: {} total, {} done",
        app.jobs.len(),
        app.jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Done))
            .count()
    );

    let status_text = if let Some(err) = &app.ui.error {
        format!("[{}] {} | ERROR: {}", screen_name, job_info, err)
    } else {
        format!("[{}] {} | {}", screen_name, job_info, app.ui.status)
    };

    let mut status_bar = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL).title("STATUS"))
        .wrap(Wrap { trim: true });

    if app.ui.error.is_some() {
        status_bar = status_bar.style(Style::default().fg(Color::Red));
    }

    f.render_widget(status_bar, main_layout.status_bar);

    // Render InputBox if active
    if let Some(input_state) = &app.input_box {
        input::render_input_box(f, input_state);
    }
}

/// Render the wizard screen
fn draw_wizard_screen(f: &mut Frame, app: &App) {
    let outer_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20), // Top margin
            Constraint::Min(10),        // Main content
            Constraint::Percentage(20), // Bottom margin
        ])
        .split(f.area());

    let step_num = app.wizard_state.get_step_number();
    let total_steps = app.wizard_state.total_steps;
    let prompt = app.wizard_state.get_prompt();

    let content_text = format!(
        "=== Initial Setup Wizard ===\n\nStep {}/{}\n\n{}",
        step_num, total_steps, prompt
    );

    let content = Paragraph::new(content_text)
        .block(Block::default().borders(Borders::ALL).title("Setup"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(content, outer_layout[1]);

    // Status bar for errors
    if let Some(err) = &app.ui.error {
        let error_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(f.area());

        let error_text = Paragraph::new(format!("ERROR: {}", err))
            .block(Block::default().borders(Borders::ALL).title("Error"))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });

        f.render_widget(error_text, error_layout[1]);
    }
}

/// Get help text for the current screen
fn get_help_text(screen: &Screen) -> String {
    match screen {
        Screen::Main => "q=quit | r=refresh | t=settings | Enter=edit | ↑↓=navigate".into(),
        Screen::Settings => {
            "i=input folder | o=output folder | p=template | n=name | Enter=save | ESC=cancel"
                .into()
        }
        Screen::EditJob => {
            "e=edit field | Tab=next field | m=month | Enter=commit | ESC=cancel".into()
        }
        Screen::InitialSetup => "Enter=proceed | ESC=exit wizard | Ctrl+C=quit".into(),
    }
}
