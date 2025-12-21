# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.
思考は英語で行い、回答は日本語で行ってください。

## Project Overview

receipt_tuiは、Google Drive上の領収書画像を読み込み、Google Sheetsの経費精算書テンプレートにデータを記入し、PDFとしてエクスポートするTUIアプリケーションです。Rust 2024エディションで書かれており、ratatui/crosstermによるターミナルUI、tokioによる非同期処理、yup-oauth2によるGoogle OAuth認証を使用しています。

## Build and Development Commands

### Using mise (Recommended)
```bash
mise run fmt          # Format code with rustfmt
mise run fmt-check    # Check formatting (CI-friendly)
mise run lint         # Run clippy with warnings as errors
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
- **`app.rs`**: メインイベントループとTUI状態管理。`App`構造体がアプリケーション状態を保持し、キーボード入力を処理してワーカーにコマンドを送信
- **`ui.rs`**: ターミナル初期化/復元のユーティリティ
- **`events.rs`**: UI状態定義（`Screen`列挙型、`UiState`構造体）
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
WorkerCmd via mpsc::Sender
    ↓
Worker (worker.rs) → Google APIs (google/)
    ↓
WorkerEvent via mpsc::Sender
    ↓
App updates (app.rs) → UI redraw (ratatui)
```

### Key Patterns

1. **Channel-based concurrency**: UIスレッドとワーカースレッドは直接状態を共有せず、チャネル経由でメッセージをやり取り
2. **State machine UI**: `Screen`列挙型（Main/Settings/EditJob）で画面遷移を管理
3. **Job lifecycle**: `JobStatus`がQueued → WaitingUserFix → WritingSheet → ExportingPdf → UploadingPdf → Doneと遷移
4. **Config persistence**: `Config`構造体はTOML形式で`config.toml`に永続化され、ワーカーに`SaveSettings`コマンドで渡される

### Google Sheets Integration Details

- `worker.rs:commit_one`関数がジョブのコミット処理全体を実行
- テンプレートシートをコピーして新しいシートを作成
- `config.template`のセル位置（`name_cell`、`target_month_cell`）に名前と対象月を記入
- `config.general_expense`の設定（`start_row`、各列）に従って既存行数をカウントし、次の空行に領収書データを追加
- シートをPDFにエクスポートし、指定フォルダにアップロード

## Configuration

`config.toml`（実行時に自動生成、gitignore済み）の構造:

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

## Testing

テストフレームワークはまだ設定されていません。テストを追加する場合:
- 単体テスト: 各モジュールファイル内に`#[cfg(test)]`モジュールを追加
- 統合テスト: `tests/`ディレクトリに配置（例: `tests/drive_smoke.rs`）
- 実行: `cargo test`

## Important Notes

- `credentials.json`、`token.json`、`config.toml`はローカルのみに保持（`.gitignore`に含まれている）
- OAuth認証スコープ: `https://www.googleapis.com/auth/drive`、`https://www.googleapis.com/auth/spreadsheets`
- TUIは50msポーリングでキーボードイベントをチェックし、ワーカーイベントを`try_recv`で非ブロッキング処理
- `prompt`関数はraw modeを一時的に無効化してユーザー入力を取得（Settings/EditJob画面で使用）
