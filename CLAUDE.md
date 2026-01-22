# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.
思考は英語で行い、回答は日本語で行ってください。

## Project Overview

receipt_tuiは、Google Drive上の領収書画像を読み込み、Google Sheetsの経費精算書テンプレートにデータを記入し、PDFとしてエクスポートするTUIアプリケーションです。Rust 2024エディションで書かれており、ratatui/crosstermによるターミナルUI、tokioによる非同期処理、yup-oauth2によるGoogle OAuth認証を使用しています。

## Build and Development Commands

### Using mise (Recommended)
```bash
# Development
mise run dev          # Start the TUI application
mise run run          # Alias for 'dev'
mise run watch        # Auto-restart on file changes (requires cargo-watch)
mise run check        # Fast type-check without building

# Building
mise run build        # Build optimized release binary
mise run build-dev    # Build debug binary

# Code Quality
mise run format       # Format code with rustfmt (alias: fmt)
mise run fmt-check    # Check formatting without modifying files
mise run lint         # Run clippy and format check
mise run fix          # Auto-fix clippy warnings

# Testing
mise run test         # Run all tests
mise run test-verbose # Run tests with output visible

# CI/CD
mise run ci           # Run full CI pipeline (format, lint, test, build)

# Documentation
mise run doc          # Generate and open project documentation
mise run doc-all      # Generate docs including dependencies

# Maintenance
mise run clean        # Remove build artifacts
mise run deps-update  # Update dependencies in Cargo.lock
mise run tree         # Show dependency tree
```

### Using cargo directly
```bash
cargo run             # Start the TUI application
cargo check           # Type-check without building binary
cargo build           # Build the binary
cargo test            # Run all tests (when tests are added)
```

### First Run Setup
アプリケーション初回起動時には、プロジェクトルートに`credentials.json`（Google Cloud Consoleから取得したOAuthクレデンシャル）が必要です。初回実行時にブラウザでOAuth認証フローが開き、`token.json`が生成されます。以降はこのトークンが再利用されます。

## Architecture

### Module Structure

このアプリケーションは、UIスレッドとワーカースレッドが`tokio::mpsc`チャネルで通信する非同期アーキテクチャを採用しています。

- **`main.rs`**: エントリーポイント。tokioランタイムを起動してアプリケーションを実行
- **`app/`**: メインイベントループとTUI状態管理
  - **`mod.rs`**: `App`構造体の定義とメインループ（`run_app`関数）
  - **`handlers.rs`**: キーボード入力のハンドラー関数群（画面ごとのキー処理）
  - **`render.rs`**: 描画ロジック（`draw`関数で4ペインレイアウトを構築）
- **`ui.rs`**: ターミナル初期化/復元のユーティリティ
- **`shortcuts.rs`**: ショートカットキー設定の読み込みと解析。`shortcut.toml`からキーバインディングをロード
- **`events.rs`**: UI状態定義（`Screen`列挙型、`UiState`構造体）
- **`input.rs`**: TUI内での文字列入力コンポーネント（InputBox）。raw modeを維持したまま、ポップアップ形式で入力を受け付ける
- **`layout.rs`**: レイアウト計算のヘルパー関数。4ペイン（Jobs Table + INFO Panel + HELP + STATUS）のレイアウトを管理
- **`wizard.rs`**: 初期設定ウィザードのステート管理。7つのステップでユーザーをガイド
- **`worker.rs`**: バックグラウンドワーカースレッド。`WorkerCmd`を受信し、Google APIを呼び出して`WorkerEvent`をUIに送信
- **`jobs.rs`**: ジョブモデル（`Job`、`JobStatus`、`ReceiptFields`）
- **`config.rs`**: `config.toml`の読み込み/保存。Google Folder/Sheet ID、ユーザー名、テンプレート設定などを管理
- **`google/`**: Google API統合
  - **`auth.rs`**: yup-oauth2を使用したOAuth認証。`credentials.json`と`token.json`を使用
  - **`drive.rs`**: Drive API操作（フォルダ内画像一覧取得、ファイルコピー、PDF export/upload）
  - **`sheets.rs`**: Sheets API操作（シート情報取得、セル値の一括更新）

### Communication Flow

```
User Input (app.rs)
    ↓
InputBox (input.rs) or Direct Key Handler
    ↓
WorkerCmd via mpsc::Sender
    ↓
Worker (worker.rs) → Google APIs (google/)
    ↓
WorkerEvent via mpsc::Sender
    ↓
App updates (app.rs) → UI redraw (ratatui)
    ↓
4-pane layout (layout.rs) + InputBox overlay (input.rs)
```

### Key Patterns

1. **Channel-based concurrency**: UIスレッドとワーカースレッドは直接状態を共有せず、チャネル経由でメッセージをやり取り
2. **State machine UI**: `Screen`列挙型（Main/Settings/EditJob/InitialSetup）で画面遷移を管理
3. **InputBox component**: raw modeを維持したまま、TUI内でポップアップ形式の入力を実現。ESCでキャンセル、Enterで確定
4. **Initial setup wizard**: 初回起動時に7ステップのウィザードでユーザーをガイド（Welcome → CheckAuth → InputFolderId → OutputFolderId → TemplateSheetId → UserName → Complete）
5. **Job lifecycle**: `JobStatus`がQueued → WaitingUserFix → WritingSheet → ExportingPdf → UploadingPdf → Doneと遷移
6. **Config persistence**: `Config`構造体はTOML形式で`config.toml`に永続化され、ワーカーに`SaveSettings`コマンドで渡される
7. **Settings buffer management**: Settings画面でESC時にバッファをリセットし、前回の編集値を破棄
8. **4-pane layout**: Jobs Table (70%) + INFO Panel (30%) + HELP Bar + STATUS Bar の4ペイン構成
9. **Auto-generated target month**: `edit_target_month`は起動時に現在の年月で自動生成（ハードコーディングなし）
10. **Customizable shortcuts**: `shortcut.toml`でキーバインディングをカスタマイズ可能。`shortcuts.rs`が設定を読み込み、各ハンドラーで使用

### Google Sheets Integration Details

- `worker.rs:commit_one`関数がジョブのコミット処理全体を実行
- テンプレートシートをコピーして新しいシートを作成
- `config.template`のセル位置（`name_cell`、`target_month_cell`）に名前と対象月を記入
- `config.general_expense`の設定（`start_row`、各列）に従って既存行数をカウントし、次の空行に領収書データを追加
- シートをPDFにエクスポートし、指定フォルダにアップロード

## Configuration

### config.toml
アプリケーション設定ファイル（実行時に自動生成、gitignore済み）の構造:

```toml
[google]
input_folder_id = ""      # Drive folder containing receipt images
output_folder_id = ""     # Drive folder for exported PDFs
template_sheet_id = ""    # Google Sheets template ID

[user]
full_name = "Your Name"

[template]
name_cell = "F3"          # Cell for user name
target_month_cell = "B3"  # Cell for target month (YYYY-MM-DD format)

[general_expense]
start_row = 44            # First row for expense entries
date_col = "B"            # Column for date
reason_col = "C"          # Column for reason
amount_col = "D"          # Column for amount
category_col = "E"        # Column for category
note_col = "F"            # Column for note
```

### shortcut.toml
キーバインディング設定ファイル（gitで管理、ユーザーがカスタマイズ可能）。各画面（main、settings、edit_job、wizard、input_box）ごとにキー操作を定義します。キーは`["Char(r)"]`、`["Char(q)"]`、`["Enter"]`などの形式で記載します。

## Testing

テストフレームワークはまだ設定されていません。テストを追加する場合:
- 単体テスト: 各モジュールファイル内に`#[cfg(test)]`モジュールを追加
- 統合テスト: `tests/`ディレクトリに配置（例: `tests/drive_smoke.rs`）
- 実行: `cargo test`

## Important Notes

- `credentials.json`、`token.json`、`config.toml`はローカルのみに保持（`.gitignore`に含まれている）
- OAuth認証スコープ: `https://www.googleapis.com/auth/drive`、`https://www.googleapis.com/auth/spreadsheets`
- TUIは50msポーリングでキーボードイベントをチェックし、ワーカーイベントを`try_recv`で非ブロッキング処理
- **InputBox**: raw modeを維持したまま、TUI内で文字列入力を実現。`input.rs`モジュールで実装
  - サポートされるキー操作: Enter（確定）、ESC（キャンセル）、Backspace、Delete、Left/Right、Home/End、Ctrl+U（行クリア）
  - 横スクロール対応（長いフォルダIDやシートIDの入力に対応）
  - カーソル位置を`|`文字で視覚的に表現
- **初期起動ウィザード**: `config.toml`が空または必須項目が未設定の場合、InitialSetup画面から起動
  - credentials.json の存在確認 → フォルダID・シートID・ユーザー名の入力 → 設定保存
  - ESCでステップをスキップ可能
  - 必須項目が空の場合、完了ステップでバリデーションエラー
- **HELP/STATUSバー**: 各画面で利用可能なキーバインディングをHELPバーに表示。STATUSバーには画面名、ジョブ情報、エラーを表示
- **Settings画面のバッファ管理**: ESC時にバッファをリセットし、保存済みのconfig値を再ロード（前回の編集値を破棄）
