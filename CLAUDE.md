# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A small Rust CLI (Linux) that displays Claude Code and optional Codex usage as a [Waybar](https://github.com/Alexays/Waybar) module. There is no separate backend — it reads local OAuth credentials and polls Anthropic/ChatGPT usage endpoints directly.

It runs **one-shot**: poll once, print a single line of Waybar JSON (`text`/`tooltip`/`class`/`percentage`) to stdout, exit. Waybar's `interval`/`signal` drive the cadence; coloring is left to Waybar CSS via the emitted `class`, plus inline Pango spans that recolor the percentage digits by severity.

This is a Linux port of the Windows taskbar widget [CodeZeno/Claude-Code-Usage-Monitor](https://github.com/CodeZeno/Claude-Code-Usage-Monitor) by Craig Constable (MIT). The original Windows GUI has been removed from this fork — it lives in git history and upstream if ever needed. The polling/credential/localization/usage-formatting logic is reused from it. Attribution is in `LICENSE`, `NOTICE`, and the README Credits.

## Commands

```bash
cargo build              # debug build
cargo build --release    # optimized (opt-level=z, lto, strip, panic=abort)
cargo clippy
cargo fmt

claude-code-usage-monitor | jq          # emit one line of Waybar JSON
claude-code-usage-monitor --help        # flags + config-file docs
claude-code-usage-monitor --diagnose    # run with logging to $TMPDIR/claude-code-usage-monitor.log
```

There is no `build.rs`. Bumping the version is just editing `version` in `Cargo.toml`.

## Architecture

`main.rs` parses `--diagnose`, then calls `waybar::run(&args)` and exits with its return code.

### Modules

- **`waybar.rs`** — the front-end. Resolves config (defaults ← `~/.config/claude-code-usage-monitor/config.json` ← CLI flags), calls `poller::poll`, and prints one line of Waybar JSON. Maps `PollError::{NoCredentials,AuthRequired,TokenExpired}` → `class: "auth-required"` (stays visible), `RequestFailed` → `"error"`; otherwise `ok`/`warning`/`critical` by max percentage. `--window session|weekly|both` picks what the bar text shows (the tooltip always shows both with countdowns). `colored_pct` wraps each percentage in a value-coloured Pango `<span>` (green→yellow→orange→red); the surrounding glyph/labels keep the module's CSS color.
- **`poller.rs`** — all network + credential logic. Reads Claude credentials from `~/.claude/.credentials.json` and Codex credentials from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`. Fetches usage, formats countdown text (`format_line` / `format_countdown`, both used by `waybar.rs`). Contains a hand-rolled ISO-8601/unix datetime parser to avoid pulling in `chrono`/`time`.
- **`models.rs`** — plain data structs: `UsageSection` (percentage + reset time), `UsageData` (session/5h + weekly/7d), `AppUsageData` (claude_code + codex options).
- **`localization/`** — 8 languages. Each `<lang>.rs` exports a `STRINGS: Strings` const; `mod.rs` defines the `Strings` struct, the `LanguageId` enum (`from_code` / `strings`), and `$LANG`/`LC_*` locale detection. **Adding a UI string means adding a field to `Strings` and filling it in all 8 language files** — they will not compile otherwise. Many `Strings` fields are inherited from the Windows GUI and currently unused, so the struct carries `#[allow(dead_code)]`.
- **`diagnose.rs`** — opt-in file logger gated on `--diagnose`, writing to `$TMPDIR/claude-code-usage-monitor.log`.

### Key flows

**Usage source fallback.** `fetch_usage_with_fallback` tries the dedicated `…/oauth/usage` endpoint first; if reset timers are missing it backfills them, and if that endpoint is unavailable entirely it falls back to reading `anthropic-ratelimit-unified-*` headers from a minimal `/v1/messages` call (iterating `MODEL_FALLBACK_CHAIN`). 401/403 surface as `PollError::AuthRequired`.

**Token refresh.** The app never writes credential files. On an expired/auth-failed token it shells out to the local `claude`/`codex` CLI (`claude -p .`, `codex exec .`) to trigger that CLI's own refresh, then re-reads the file. The spawn is bounded by `REFRESH_TIMEOUT` (8s) so a one-shot poll never stalls the bar. See `refresh_or_fallback` and `cli_refresh_*`.

**Config.** `~/.config/claude-code-usage-monitor/config.json` (via `dirs::config_dir()`), mapped to `FileConfig` (serde, all fields optional). CLI flags (`--claude`/`--no-claude`, `--codex`/`--no-codex`, `--window`, `--lang`) override it. Defaults work with zero config; `build_config` errors only if both providers end up disabled.

## Conventions

- No async runtime and deliberately minimal dependencies (`ureq`, `serde`, `serde_json`, `dirs`, `native-tls`). Prefer hand-rolled solutions over new crates — the binary size profile (`opt-level="z"`, LTO) is intentional. The datetime parser in `poller.rs` exists for this reason.
- Spawned child processes (`claude`/`codex` refresh) use null stdio and are bounded by a timeout loop so a poll can't hang.
- UI text is value-neutral data; coloring/styling belongs in Waybar CSS or the inline Pango spans, not hardcoded ANSI.
