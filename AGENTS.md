# Repository Guidelines

思考は英語で行い、回答は日本語で行ってください。

## Project Structure & Module Organization
- `src/` contains the Rust application. Key modules:
  - `app.rs` (TUI flow), `ui.rs` (terminal setup), `worker.rs` (Google API jobs)
  - `google/` (OAuth, Drive, Sheets client helpers)
  - `config.rs` (config load/save), `jobs.rs` (job model)
- `config.toml` is generated at runtime and stores user and Google IDs.
- No test directory yet; add tests under `tests/` for integration or `src/*` unit tests.

## Build, Test, and Development Commands
Use `mise` tasks:
- `mise run fmt`: format all Rust code with `rustfmt`.
- `mise run fmt-check`: CI-friendly formatting check.
- `mise run lint`: run `clippy` with warnings as errors.

Common cargo commands:
- `cargo run`: start the TUI (requires `credentials.json` at repo root).
- `cargo check`: type-check without building the binary.

## Coding Style & Naming Conventions
- Rust 2024 edition; follow standard `rustfmt` formatting (4-space indentation, trailing commas).
- Use `snake_case` for functions/modules, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Prefer short, explicit names in UI state and worker events.

## Testing Guidelines
- No testing framework is set up yet. If you add tests, use:
  - Unit tests in `src/*.rs` with `#[cfg(test)]`.
  - Integration tests in `tests/` (e.g., `tests/drive_smoke.rs`).
- Run tests with `cargo test`.

## Commit & Pull Request Guidelines
- No commit history or conventions are defined yet. Use a clear, imperative subject line (e.g., “Add Drive upload status updates”).
- PRs should include:
  - A short summary of changes.
  - Steps to run locally (`cargo run`, `mise run lint`, etc.).
  - Any relevant config notes (e.g., new `config.toml` keys).

## Security & Configuration Tips
- Keep `credentials.json`, `token.json`, and `config.toml` local (already in `.gitignore`).
- The OAuth flow opens a browser on first run; reuses `token.json` after that.
