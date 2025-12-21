//! TUI event loop, input handling, and rendering.

use anyhow::Result;
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
    jobs::{Job, JobStatus},
    ui::Tui,
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

    let mut app = App {
        cfg_path,
        cfg: cfg.clone(),
        ui: UiState {
            screen: Screen::Main,
            selected: 0,
            log: vec![],
            status: "Ready".into(),
            editing_field_idx: 0,
        },
        jobs: vec![],
        worker_tx: tx_cmd,
        worker_rx: rx_ev,
        in_folder: cfg.google.input_folder_id.clone(),
        out_folder: cfg.google.output_folder_id.clone(),
        template_id: cfg.google.template_sheet_id.clone(),
        full_name: cfg.user.full_name.clone(),
        edit_target_month: "2025-12".into(),
    };

    // Initial refresh so the UI has data when possible.
    request_refresh(&mut app).await?;

    loop {
        // Render the current state to the terminal.
        terminal.draw(|f| draw(f, &app))?;

        // Drain worker events before handling the next input.
        while let Ok(ev) = app.worker_rx.try_recv() {
            handle_worker_event(&mut app, ev)?;
        }

        // Poll for input with a small timeout to keep the UI responsive.
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                if handle_key(&mut app, k).await? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Apply a worker event to the UI state.
fn handle_worker_event(app: &mut App, ev: WorkerEvent) -> Result<()> {
    match ev {
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

/// Handle a single key press; returns true when the app should exit.
async fn handle_key(app: &mut App, k: KeyEvent) -> Result<bool> {
    match app.ui.screen {
        // Main screen shortcuts and selection movement.
        Screen::Main => match k.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('t') => {
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
        },

        // Settings input uses single-letter shortcuts with inline prompts.
        Screen::Settings => match k.code {
            KeyCode::Esc => app.ui.screen = Screen::Main,
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
                app.in_folder = prompt("Input folder id: ")?;
            }
            KeyCode::Char('o') => {
                app.out_folder = prompt("Output folder id: ")?;
            }
            KeyCode::Char('p') => {
                app.template_id = prompt("Template sheet id: ")?;
            }
            KeyCode::Char('n') => {
                app.full_name = prompt("Full name: ")?;
            }
            _ => {}
        },

        // EditJob lets the user tweak fields before committing to Sheets.
        Screen::EditJob => match k.code {
            KeyCode::Esc => app.ui.screen = Screen::Main,
            KeyCode::Tab => app.ui.editing_field_idx = (app.ui.editing_field_idx + 1) % 5,
            KeyCode::Enter => {
                // Clone the selected job to avoid borrowing issues.
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
                app.edit_target_month = prompt("Target month (YYYY-MM): ")?;
            }
            KeyCode::Char('e') => {
                if let Some(j) = app.jobs.get_mut(app.ui.selected) {
                    match app.ui.editing_field_idx {
                        0 => j.fields.date_ymd = prompt("Date (YYYY-MM-DD): ")?,
                        1 => j.fields.reason = prompt("Reason: ")?,
                        2 => j.fields.amount_yen = prompt("Amount (yen): ")?.parse().unwrap_or(0),
                        3 => j.fields.category = prompt("Category: ")?,
                        4 => j.fields.note = prompt("Note: ")?,
                        _ => {}
                    }
                }
            }
            _ => {}
        },
    }
    Ok(false)
}

/// Temporarily disable raw mode to read a line from stdin.
fn prompt(msg: &str) -> Result<String> {
    use std::io::{self, Write};
    crossterm::terminal::disable_raw_mode()?;
    print!("\n{msg}");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    crossterm::terminal::enable_raw_mode()?;
    Ok(s.trim().to_string())
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
    // Layout: main body and a status bar at the bottom.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(f.area());

    // Split body into a jobs table (left) and info panel (right).
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(layout[0]);

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
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    // Keep the highlight on the current selection.
    let mut state = ratatui::widgets::TableState::default();
    if !app.jobs.is_empty() {
        state.select(Some(app.ui.selected));
    }
    f.render_stateful_widget(table, body[0], &mut state);

    // Context-sensitive help text per screen.
    let help = match app.ui.screen {
        Screen::Main => "Main: r=refresh, Enter=edit, t=settings, q=quit",
        Screen::Settings => "Settings: i/o/p/n to edit fields, Enter=save, Esc=back",
        Screen::EditJob => "Edit: e=edit field, Tab=next, m=month, Enter=commit, Esc=back",
    };
    // Show selected file info or placeholders when empty.
    let (sel_name, sel_id) = if let Some(j) = app.jobs.get(app.ui.selected) {
        (j.filename.clone(), j.drive_file_id.clone())
    } else {
        ("-".into(), "-".into())
    };
    // Right panel shows help, selection info, settings, and recent logs.
    let right = Paragraph::new(format!(
        "{}\n\nSelected:{}\nSelected ID:{}\nIn:{}\nOut:{}\nTpl:{}\nName:{}\nMonth:{}\n\nLog:\n{}",
        help,
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
    ))
    .block(Block::default().borders(Borders::ALL).title("INFO"))
    .wrap(Wrap { trim: true });
    f.render_widget(right, body[1]);

    // Bottom status bar for one-line feedback.
    let status = Paragraph::new(app.ui.status.clone())
        .block(Block::default().borders(Borders::ALL).title("STATUS"))
        .wrap(Wrap { trim: true });
    f.render_widget(status, layout[1]);
}
