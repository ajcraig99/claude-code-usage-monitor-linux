# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A native Windows taskbar widget (Rust, ~6k LOC) that displays Claude Code and optional Codex usage. It embeds a child window directly into the Windows taskbar (`Shell_TrayWnd`) plus one or more system tray icons. There is no separate backend — it reads local OAuth credentials and polls Anthropic/ChatGPT usage endpoints directly.

**This is a Windows-only app.** It links the `windows` crate against many Win32 APIs (`Win32_UI_WindowsAndMessaging`, GDI, Registry, Shell, WSL via `wsl.exe`). It cannot build or run on Linux/macOS — `cargo build` will fail off Windows. Edits can be made anywhere, but compilation, `cargo test`, and `cargo clippy` must happen on a Windows host or Windows CI.

## Commands

```powershell
cargo build              # debug build
cargo build --release    # optimized; this is what CI ships (opt-level=z, lto, strip, panic=abort)
cargo clippy
cargo fmt

claude-code-usage-monitor --diagnose   # run with logging to %TEMP%\claude-code-usage-monitor.log
```

Releases are tag-driven: pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds the release `.exe`, creates a GitHub Release, and submits a WinGet manifest update. **Bumping the version means editing `version` in `Cargo.toml` and tagging** — `build.rs` embeds that version into the PE resource, and `updater.rs` compares against it for self-update.

## Architecture

The app is a classic single-threaded Win32 message-pump program. `main.rs` handles `--diagnose` and the `--apply-update` CLI mode (see below), then calls `window::run()`.

### Modules

- **`window.rs`** (largest, ~2800 lines) — the heart. Owns the global `AppState` (behind `static STATE: Mutex<Option<AppState>>`, accessed via `lock_state()`), creates the layered child window, runs the message loop, and contains `wnd_proc` (the WndProc dispatching all `WM_*` messages). Handles painting, taskbar embedding/positioning, drag-to-move, the right-click context menu (all `IDM_*` command IDs), DPI scaling (`sc()` / `CURRENT_DPI`), theme changes, and settings persistence.
- **`poller.rs`** — all network + credential logic. Reads Claude credentials (`~/.claude/.credentials.json` on Windows, or via `wsl.exe` from each installed WSL distro) and Codex credentials (`$CODEX_HOME/auth.json` or `~/.codex/auth.json`). Fetches usage, formats countdown text. Contains a hand-rolled ISO-8601/unix datetime parser to avoid pulling in `chrono`/`time`.
- **`tray_icon.rs`** — builds tray `HICON`s at runtime (GDI-drawn percentage badges with interpolated fill colors), and `Shell_NotifyIconW` add/update/remove/sync. Claude and Codex are separate icons (IDs 1 and 2).
- **`updater.rs`** — GitHub-release self-update (portable installs) and WinGet upgrade path. Detects install channel by checking whether the exe lives under a WinGet install root.
- **`native_interop.rs`** — thin Win32 wrappers (taskbar lookup, window styles, WinEvent hooks, `wide_str`, `Color`).
- **`theme.rs`** — reads `SystemUsesLightTheme` from the registry for dark/light mode.
- **`models.rs`** — plain data structs: `UsageSection` (percentage + reset time), `UsageData` (session/5h + weekly/7d), `AppUsageData` (claude_code + codex options).
- **`localization/`** — 8 languages. Each `<lang>.rs` exports a `STRINGS: Strings` const; `mod.rs` defines the `Strings` struct, `LanguageId` enum, and Windows locale detection. **Adding a UI string means adding a field to `Strings` and filling it in all 8 language files** — they will not compile otherwise.
- **`diagnose.rs`** — opt-in file logger gated on `--diagnose`.

### Key flows

**Polling.** A `TIMER_POLL` (default 15 min, configurable via the menu) and the initial startup both spawn a background `std::thread` running `do_poll`, which calls `poller::poll(show_claude_code, show_codex)`. Worker threads never touch `AppState` directly — they post `WM_APP_USAGE_UPDATED` back to the window so the UI thread reads/updates state. This thread-posts-message pattern (`PostMessageW` + custom `WM_APP_*` messages) is used throughout for all async work (polling, update checks, downloads). `HWND` is moved across threads via the `SendHwnd(isize)` wrapper.

**Usage source fallback.** `fetch_usage_with_fallback` tries the dedicated `…/oauth/usage` endpoint first; if reset timers are missing it backfills them, and if that endpoint is unavailable entirely it falls back to reading `anthropic-ratelimit-unified-*` headers from a minimal `/v1/messages` call (iterating `MODEL_FALLBACK_CHAIN`). 401/403 surface as `PollError::AuthRequired`.

**Token refresh.** The app never writes credential files. On an expired/auth-failed token it shells out to the local `claude`/`codex` CLI (`claude -p .`, `codex exec .`, or the WSL equivalent) to trigger that CLI's own refresh, then re-reads the file. See `refresh_or_fallback` and `cli_refresh_*`.

**Self-update.** `updater::handle_cli_mode` makes the app re-exec itself with `--apply-update <target> <source> <pid>` as a tiny helper process that waits for the parent to exit, swaps the binary, and relaunches. WinGet installs use `winget upgrade` via a detached PowerShell process instead.

**Settings.** `%APPDATA%\ClaudeCodeUsageMonitor\settings.json`, mapped to `SettingsFile` (serde). `load_settings` enforces that at least one model is enabled. `save_state_settings()` snapshots the live `AppState` to disk after any user change.

## Conventions

- No async runtime and deliberately minimal dependencies (`ureq`, `serde`, `dirs`, `windows`, `native-tls`). Prefer hand-rolled solutions over new crates — the binary size profile (`opt-level="z"`, LTO) is intentional. The datetime parser in `poller.rs` exists for this reason.
- All spawned child processes use `creation_flags(CREATE_NO_WINDOW)` and null stdio to stay invisible; long-running spawns are bounded by `run_with_timeout` / `wait_for_refresh` so the poll thread can't hang.
- WSL output is UTF-16LE; decode it with `decode_wsl_text`, not `from_utf8`.
- UI dimensions are authored at 96 DPI and scaled through `sc()`; don't hardcode pixel positions.
