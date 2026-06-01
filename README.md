![Platform](https://img.shields.io/badge/platform-Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# Claude Code Usage Monitor — Waybar

A lightweight [Waybar](https://github.com/Alexays/Waybar) module that shows how
much of your Claude Code (and optionally Codex) usage window you have left,
without opening a terminal or the provider site.

It runs as a one-shot CLI: poll once, print a single line of Waybar JSON, exit.
Waybar drives the refresh cadence, and coloring is handled in your Waybar CSS.

This is a Linux port of
[CodeZeno/Claude-Code-Usage-Monitor](https://github.com/CodeZeno/Claude-Code-Usage-Monitor),
a native Windows taskbar widget by Craig Constable. The polling, credential,
localization, and usage-formatting logic is reused here; the Windows GUI itself
is not part of this fork. See [Credits](#credits).

## What You Get

- A **5h** (session) percentage for your current 5-hour Claude usage window
- A **7d** (weekly) percentage for your current 7-day window
- Optional Codex usage alongside Claude Code
- A countdown until each limit resets, in your locale's language
- A Waybar `class` (`ok` / `warning` / `critical` / `auth-required` / `error`)
  you can style however you like

## Requirements

- Linux with Waybar
- Claude Code (CLI) installed and authenticated (`~/.claude/.credentials.json`)
- Optional: Codex CLI installed and authenticated, for Codex usage

## Build

```bash
cargo build --release
# binary at target/release/claude-code-usage-monitor
```

Install it somewhere on your `PATH` (e.g. `~/.local/bin/`).

## Use

Run it directly to see the JSON it emits:

```bash
claude-code-usage-monitor | jq
# {"text":"16% · 1h","tooltip":"Claude Code\n  5h: 16% · 1h\n  7d: 22% · 6d","class":"ok","percentage":16}
```

### Waybar configuration

Add a `custom` module to your Waybar config:

```jsonc
"custom/claude": {
    "exec": "claude-code-usage-monitor",
    "return-type": "json",
    "interval": 900,
    "signal": 8,
    "tooltip": true
}
```

Then style it by `class` in your Waybar CSS:

```css
#custom-claude.ok            { color: #a6e3a1; }
#custom-claude.warning       { color: #f9e2af; }
#custom-claude.critical      { color: #f38ba8; }
#custom-claude.auth-required { color: #89b4fa; }
#custom-claude.error         { color: #6c7086; }
```

To force a refresh on demand, send the module's signal:
`pkill -RTMIN+8 waybar`.

### Options

```text
--claude / --no-claude       Show or hide Claude Code usage (default: show)
--codex / --no-codex         Show or hide Codex usage (default: hide)
--window <session|weekly|both>  Which window(s) the bar text shows (default: session)
--lang <code>                UI language (e.g. en, de, ja); default: $LANG
--diagnose                   Write a debug log to the temp directory
-h, --help                   Show help
```

### Config file

Defaults work with zero configuration. To persist settings, create
`$XDG_CONFIG_HOME/claude-code-usage-monitor/config.json` (usually
`~/.config/claude-code-usage-monitor/config.json`):

```json
{
  "claude": true,
  "codex": false,
  "window": "session",
  "language": null
}
```

CLI flags override the config file. `"language": null` (or omitting it) uses
your locale from `LC_ALL` → `LC_MESSAGES` → `LANG`.

## Diagnostics

```bash
claude-code-usage-monitor --diagnose
```

This writes a log to `$TMPDIR/claude-code-usage-monitor.log` (typically under
`/tmp`).

## Privacy And Security

This project is **open source**, so you can inspect exactly what it does.

What it reads:

- Your local Claude Code OAuth credentials from `~/.claude/.credentials.json`
- If Codex is enabled, your local Codex credentials from `$CODEX_HOME/auth.json`
  or `~/.codex/auth.json`

What it sends over the network:

- Requests to Anthropic's Claude endpoints to read your usage and rate-limit
  information
- Requests to ChatGPT's Codex usage endpoint, if Codex is enabled
- If proxy environment variables such as `HTTPS_PROXY`, `HTTP_PROXY`, or
  `ALL_PROXY` are set, those outbound requests may use that proxy

What it does **not** do:

- It does not send your credentials to any other server
- It does not use a separate backend service
- It does not collect analytics or telemetry
- It does not upload your project files
- It does not write your credential files itself

Notes:

- If your Claude Code or Codex token is expired, the app asks the local
  `claude` / `codex` CLI to refresh it in the background (with a short timeout so
  it never stalls the bar). The CLI performs the refresh and rewrites the file.
- Proxies should be trusted, because proxied usage requests include your OAuth
  bearer token inside the TLS connection.

## How It Works

1. Reads your Claude Code (and optionally Codex) credentials
2. Reads your current usage from Anthropic and/or ChatGPT
3. Prints one line of Waybar JSON and exits

If the dedicated usage endpoint is unavailable, it falls back to reading the
rate-limit headers returned by Claude's Messages API.

## License & credits

MIT — see [`LICENSE`](LICENSE). Linux/Waybar port of the MIT-licensed
[Claude Code Usage Monitor](https://github.com/CodeZeno/Claude-Code-Usage-Monitor)
by Craig Constable, whose polling/credential/localization logic it reuses; see
[`NOTICE`](NOTICE) for attribution.
