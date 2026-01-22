//! キー入力ハンドラー関数。

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    events::Screen,
    input::{InputBoxState, InputCallbackId},
    shortcuts,
    wizard::WizardStep,
    worker::WorkerCmd,
};

use super::{App, request_refresh};

/// キー入力を1件処理し、終了すべきならtrueを返す。
pub async fn handle_key(app: &mut App, k: KeyEvent) -> Result<bool> {
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

/// Ctrl+Cかどうかを判定する。
pub fn is_ctrl_c(k: &KeyEvent) -> bool {
    k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c')
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
    } else if shortcuts::matches_shortcut(&k, &sc.enter) && app.jobs.get(app.ui.selected).is_some()
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
        apply_input_callback(app, callback_id, value).await?;
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

/// 入力ボックスのコールバックを適用する。
async fn apply_input_callback(
    app: &mut App,
    callback_id: InputCallbackId,
    value: String,
) -> Result<()> {
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
    Ok(())
}

/// 設定画面用の編集バッファを設定値から再読み込みする。
fn reload_settings_buffers(app: &mut App) {
    // 設定の現在値を編集用バッファへ反映する。
    app.in_folder = app.cfg.google.input_folder_id.clone();
    app.out_folder = app.cfg.google.output_folder_id.clone();
    app.template_id = app.cfg.google.template_sheet_id.clone();
    app.full_name = app.cfg.user.full_name.clone();
}
