# receipt-tui-vhs reference

## Key paths
- Tape: `assets/demo.tape`
- Output: `assets/demo.gif`
- Task: `mise run demo`

## Prerequisites for deterministic recordings
- `assets/credentials.json` must exist or the app will fail to start.
- `token.json` should contain a valid refresh token to avoid interactive OAuth.
- The worker initializes OAuth on startup, even before any refresh actions.

## Config state notes
- `config.toml` is created on first run.
- To show the initial setup wizard, remove or empty `config.toml` before recording.
- The wizard checks for `assets/credentials.json` and will show an error if missing.

## Stable flow guidance
- Prefer wizard + settings screens to avoid network calls.
- Avoid triggering refresh (`r`) unless the Google IDs are valid and network access is stable.
- If you must show job editing, ensure the Drive folder contains predictable test data.

## Common pitfalls
- OAuth browser prompts will stall VHS; ensure `token.json` is valid.
- Network failures can introduce nondeterministic UI errors; keep flows offline unless required.
- Long pauses are better than rapid key bursts if the TUI is slow on CI.
