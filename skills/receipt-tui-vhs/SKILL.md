---
name: receipt-tui-vhs
description: Repo-specific workflow for creating VHS demo recordings and lightweight E2E-style walkthroughs for receipt_tui. Use when asked to update `assets/demo.tape`, generate `assets/demo.gif`, or design a VHS flow for this TUI.
---

# receipt-tui-vhs

## Overview

Use this skill to create or update a VHS tape for receipt_tui and generate a deterministic demo GIF at `assets/demo.gif`.

## Workflow

1) Confirm the goal and scope
- Decide whether the user wants a visual demo, an E2E-style walkthrough, or both.
- Keep flows deterministic; avoid network-dependent steps unless explicitly requested.

2) Prepare the environment
- Ensure prerequisites are met (credentials, token, config state).
- See `references/receipt-tui-vhs.md` for repo-specific prerequisites and pitfalls.

3) Edit the tape
- Update `assets/demo.tape` to reflect the requested flow.
- Prefer short, stable sleeps and predictable screens (wizard/settings).
- Use existing shortcuts and UI flows; avoid adding new app behavior.

4) Run VHS
- Prefer `mise run demo` (configured in `mise.toml`).
- Output is `assets/demo.gif`.

5) Verify output
- Open `assets/demo.gif` and check for errors, prompts, or stuck states.
- If the tape hits OAuth or network prompts, adjust the flow to keep it deterministic.

## Resources

- `references/receipt-tui-vhs.md`: prerequisites, stable flows, and common pitfalls.
