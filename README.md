# receipt_tui

Google Drive上のレシート画像を一覧し、TUIで内容を入力してGoogleスプレッドシートに書き込み、PDFとしてDriveへアップロードするためのツールです。

## 特徴
- Driveの入力フォルダから画像を取得し、ジョブ一覧として表示
- 画面内で日付・理由・金額・カテゴリ・メモを編集
- テンプレートのスプレッドシートへ書き込み、PDFを出力してDriveへ保存
- 進行状況とログをTUI内で確認可能

## 必要なもの
- Rust (2024 edition)
- Google APIの`credentials.json`（リポジトリ直下）
- Google Drive/Sheetsへのアクセス権

## セットアップ
1. Google APIの認証情報を`credentials.json`として配置します。
2. `cargo run`で起動します（初回はブラウザでOAuthが開き、`token.json`が生成されます）。
3. TUIの設定画面で以下を入力します。
   - Input folder id（レシート画像のDriveフォルダ）
   - Output folder id（PDFの出力先フォルダ）
   - Template sheet id（テンプレートのスプレッドシートIDまたはショートカットID）
   - Full name（テンプレートに記載する氏名）

`config.toml`は初回起動時に自動生成されます。

## 使い方（キー操作）
### メイン画面
- `r`: Driveを再読み込み
- `Enter`: 選択ジョブの編集
- `t`: 設定画面へ
- `q`: 終了
- `↑/↓`: 選択移動

### 設定画面
- `i`: Input folder id を編集
- `o`: Output folder id を編集
- `p`: Template sheet id を編集
- `n`: Full name を編集
- `Enter`: 保存して戻る
- `Esc`: 戻る

### ジョブ編集画面
- `e`: 現在のフィールドを編集
- `Tab`: 次のフィールドへ
- `m`: 対象月（YYYY-MM）を変更
- `Enter`: スプレッドシートへ反映 & PDF出力
- `Esc`: 戻る

## 開発コマンド
- `mise run fmt`: フォーマット
- `mise run fmt-check`: フォーマットチェック
- `mise run lint`: clippy
- `cargo check`: 型チェック
- `cargo test`: テスト実行（追加時）

## ログ
- `receipt_tui.log` に出力されます。

## 注意
- `credentials.json` / `token.json` / `config.toml` はローカル専用です（`.gitignore`済み）。
