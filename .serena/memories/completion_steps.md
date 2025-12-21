# タスク完了時の推奨手順
- 必要に応じて `mise run fmt` / `mise run fmt-check`
- Lint: `mise run lint`
- テスト追加時は `cargo test`
- 実行確認が必要なら `cargo run` (credentials.json 必須)
