---
name: mochi-notch
description: Show a task's progress, a "needs you" alert or a "done" check in the Mochi notch on Windows, with mochi-cli. Use when the user asks to be told in the notch, or before a long task (build, tests, deploy, download, migration) on a Windows PC running Mochi, so the user can look away.
---

# Mochi notch

Mochi is a notch at the top of the Windows screen. `mochi-cli` (installed next to Mochi, usually on PATH; otherwise `%LOCALAPPDATA%\Programs\Mochi\mochi-cli.exe`) puts an *activity* in a bubble beside it: a title, a short text, an optional progress ring, and a state.

```powershell
mochi-cli activity set deploy --title "Deploy" --text "building" --progress 20% --icon build
mochi-cli activity set deploy --title "Deploy" --text "uploading" --progress 70% --icon build
mochi-cli activity done deploy --title "Deploy" --text "live"
mochi-cli activity fail deploy --title "Deploy" --text "tests failed"
mochi-cli activity attention deploy --title "Deploy" --text "approve the migration?"
mochi-cli activity end deploy
```

Rules:
- **Always pass `--title`**, with every `set`/`done`/`fail`/`attention`. A running activity nobody updates expires after 10 minutes, and an id that's gone needs its title to come back.
- Use one stable `id` per task (letters, digits, `. _ : -`), and finish it: `done` (a check, gone after 12 s), `fail` (stays until `end`), or `end`.
- `--progress` takes `0.4` or `40%`. Icons: `activity`, `code`, `build`, `download`, `upload`, `sync`, `agent`, `bell`, `timer`, and the agents' own `copilot`, `claude`, `cursor`, `codex`.
- `attention` means the user must act (red, and a notice that waits to be seen). Use it only when you are blocked on them.
- The text shows on screen: keep it short (title ≤ 60 characters, text ≤ 120) and never put secrets or private content in it.
- Exit codes: 0 shown, 1 bad arguments (fix the call), 2 Mochi isn't running (carry on without it, don't retry), 3 refused (the reason is printed).
- Nothing is printed on success. `mochi-cli activity --help` lists everything.

If the user set up the agent hooks (`mochi-cli hook setup copilot|claude|cursor|codex`), your own working / waiting / done state already shows in the notch; use activities for the tasks you run, not for yourself.
