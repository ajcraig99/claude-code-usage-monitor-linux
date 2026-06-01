//! Linux/Waybar front-end.
//!
//! This binary runs as a one-shot Waybar `custom` module: it polls usage once,
//! prints a single line of Waybar JSON (`text`/`tooltip`/`class`/`percentage`)
//! to stdout, and exits. Waybar's own `interval`/`signal` drive the cadence,
//! and coloring is left to Waybar CSS via the emitted `class`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnose;
use crate::localization::{self, LanguageId, Strings};
use crate::models::{UsageData, UsageSection};
use crate::poller::{self, PollError};

/// Which rate-limit window(s) the bar text shows. The tooltip always shows
/// both with full countdowns regardless.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Window {
    Session,
    Weekly,
    Both,
}

impl Window {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "session" | "5h" | "five_hour" => Some(Self::Session),
            "weekly" | "7d" | "seven_day" => Some(Self::Weekly),
            "both" | "all" => Some(Self::Both),
            _ => None,
        }
    }

    /// The bar text for this selection. Session/Weekly keep the reset
    /// countdown (e.g. `18% · 1h`); Both shows just the two labelled
    /// percentages (e.g. `5h 18% · 7d 22%`) to stay compact — the tooltip
    /// carries the full detail. Every percentage is wrapped in a value-
    /// coloured Pango span; the rest of the text keeps the module's CSS colour.
    fn text(self, usage: &UsageData, strings: Strings) -> String {
        match self {
            Self::Session => section_text(&usage.session, strings),
            Self::Weekly => section_text(&usage.weekly, strings),
            Self::Both => format!(
                "{} {} \u{00b7} {} {}",
                strings.session_window,
                colored_pct(usage.session.percentage),
                strings.weekly_window,
                colored_pct(usage.weekly.percentage),
            ),
        }
    }

    /// The percentage that drives `class`/`percentage` for this selection
    /// (the higher of the two windows when showing both).
    fn max_percentage(self, usage: &UsageData) -> f64 {
        match self {
            Self::Session => usage.session.percentage,
            Self::Weekly => usage.weekly.percentage,
            Self::Both => usage.session.percentage.max(usage.weekly.percentage),
        }
    }
}

/// A single window for the bar: a value-coloured percentage followed by the
/// reset countdown (e.g. `<span>18%</span> · 1h`).
fn section_text(section: &UsageSection, strings: Strings) -> String {
    let pct = colored_pct(section.percentage);
    let countdown = poller::format_countdown(section.resets_at, strings);
    if countdown.is_empty() {
        pct
    } else {
        format!("{pct} \u{00b7} {countdown}")
    }
}

/// Wrap a percentage in a Pango span coloured by severity — green → yellow →
/// orange → red as usage climbs. Waybar renders the markup inline; only the
/// digits are recoloured, so the surrounding glyph/labels keep the module's
/// CSS colour.
fn colored_pct(percentage: f64) -> String {
    let color = if percentage >= 90.0 {
        "#e5484d" // red
    } else if percentage >= 75.0 {
        "#f0883e" // orange
    } else if percentage >= 50.0 {
        "#f9e2af" // yellow
    } else {
        "#a6e3a1" // green
    };
    format!("<span foreground=\"{color}\">{percentage:.0}%</span>")
}

/// Resolved runtime configuration after merging defaults, the config file, and
/// CLI overrides.
struct Config {
    show_claude: bool,
    show_codex: bool,
    window: Window,
    language: Option<LanguageId>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_claude: true,
            show_codex: false,
            window: Window::Session,
            language: None,
        }
    }
}

/// On-disk config schema. Every field is optional so a partial (or absent)
/// file still yields a working configuration.
#[derive(Default, Deserialize)]
struct FileConfig {
    claude: Option<bool>,
    codex: Option<bool>,
    window: Option<String>,
    language: Option<String>,
}

/// The single line of JSON Waybar consumes for a `return-type: json` module.
#[derive(Serialize)]
struct WaybarOutput {
    text: String,
    tooltip: String,
    class: String,
    percentage: u8,
}

/// Entry point for the Unix build. Returns a process exit code.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return 0;
    }

    let config = match build_config(args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };

    let output = produce_output(&config);
    match serde_json::to_string(&output) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(error) => {
            diagnose::log_error("failed to serialize Waybar output", error);
            1
        }
    }
}

fn produce_output(config: &Config) -> WaybarOutput {
    let strings = localization::resolve_language(config.language).strings();

    match poller::poll(config.show_claude, config.show_codex) {
        Ok(data) => {
            let mut entries: Vec<(&'static str, &UsageData)> = Vec::new();
            if let Some(claude) = &data.claude_code {
                entries.push((strings.claude_code_model, claude));
            }
            if let Some(codex) = &data.codex {
                entries.push((strings.codex_model, codex));
            }
            build_success_output(&entries, config.window, strings)
        }
        Err(error) => build_error_output(&error, strings),
    }
}

fn build_success_output(
    entries: &[(&'static str, &UsageData)],
    window: Window,
    strings: Strings,
) -> WaybarOutput {
    let show_label = entries.len() > 1;

    let text = entries
        .iter()
        .map(|(label, usage)| {
            let line = window.text(usage, strings);
            if show_label {
                format!("{label} {line}")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("  ");

    let tooltip = entries
        .iter()
        .map(|(label, usage)| {
            format!(
                "{label}\n  {}: {}\n  {}: {}",
                strings.session_window,
                poller::format_line(&usage.session, strings),
                strings.weekly_window,
                poller::format_line(&usage.weekly, strings),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let max_percent = entries
        .iter()
        .map(|(_, usage)| window.max_percentage(usage))
        .fold(0.0_f64, f64::max);

    WaybarOutput {
        text,
        tooltip,
        class: class_for_percentage(max_percent).to_string(),
        percentage: percent_to_u8(max_percent),
    }
}

fn build_error_output(error: &PollError, strings: Strings) -> WaybarOutput {
    match error {
        // Keep the module visible and tell the user how to re-authenticate.
        PollError::NoCredentials | PollError::AuthRequired | PollError::TokenExpired => {
            WaybarOutput {
                text: "auth".to_string(),
                tooltip: format!(
                    "{}\n{}",
                    strings.token_expired_title, strings.token_expired_body
                ),
                class: "auth-required".to_string(),
                percentage: 0,
            }
        }
        PollError::RequestFailed => WaybarOutput {
            text: "err".to_string(),
            tooltip: "Failed to fetch usage. Check your network connection.".to_string(),
            class: "error".to_string(),
            percentage: 0,
        },
    }
}

fn class_for_percentage(percentage: f64) -> &'static str {
    if percentage >= 90.0 {
        "critical"
    } else if percentage >= 50.0 {
        "warning"
    } else {
        "ok"
    }
}

fn percent_to_u8(percentage: f64) -> u8 {
    percentage.round().clamp(0.0, 100.0) as u8
}

/// Merge defaults <- config file <- CLI flags. Returns an error only for
/// malformed CLI arguments; a malformed config file is logged and ignored.
fn build_config(args: &[String]) -> Result<Config, String> {
    let mut config = Config::default();

    if let Some(file) = load_file_config() {
        if let Some(claude) = file.claude {
            config.show_claude = claude;
        }
        if let Some(codex) = file.codex {
            config.show_codex = codex;
        }
        if let Some(window) = file.window.as_deref() {
            match Window::parse(window) {
                Some(window) => config.window = window,
                None => diagnose::log(format!("ignoring unknown window in config: {window}")),
            }
        }
        if let Some(language) = file.language.as_deref() {
            config.language = LanguageId::from_code(language);
        }
    }

    apply_cli_overrides(args, &mut config)?;

    if !config.show_claude && !config.show_codex {
        return Err("Nothing to display: enable at least one of Claude or Codex.".to_string());
    }

    Ok(config)
}

fn apply_cli_overrides(args: &[String], config: &mut Config) -> Result<(), String> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--claude" => config.show_claude = true,
            "--no-claude" => config.show_claude = false,
            "--codex" => config.show_codex = true,
            "--no-codex" => config.show_codex = false,
            "--window" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--window requires a value (session|weekly)".to_string())?;
                config.window = Window::parse(value)
                    .ok_or_else(|| format!("invalid --window value: {value}"))?;
            }
            "--lang" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--lang requires a value (e.g. en, de, ja)".to_string())?;
                // An unrecognised code falls back to system detection.
                config.language = LanguageId::from_code(value);
            }
            // Recognised no-ops handled elsewhere or intentionally ignored.
            "--waybar" | "--diagnose" => {}
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(())
}

fn config_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("claude-code-usage-monitor")
            .join("config.json"),
    )
}

fn load_file_config() -> Option<FileConfig> {
    let path = config_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&content) {
        Ok(config) => Some(config),
        Err(error) => {
            diagnose::log_error(
                &format!("ignoring malformed config at {}", path.display()),
                error,
            );
            None
        }
    }
}

fn print_help() {
    println!(
        "claude-code-usage-monitor — Waybar usage module\n\
         \n\
         Usage: claude-code-usage-monitor [OPTIONS]\n\
         \n\
         Prints one line of Waybar JSON (text/tooltip/class/percentage) and exits.\n\
         \n\
         Options:\n\
         \x20 --claude / --no-claude   Show or hide Claude Code usage (default: show)\n\
         \x20 --codex / --no-codex     Show or hide Codex usage (default: hide)\n\
         \x20 --window <session|weekly|both>  Which window(s) the bar text shows (default: session)\n\
         \x20 --lang <code>            UI language (e.g. en, de, ja); default: $LANG\n\
         \x20 --diagnose               Write a debug log to the temp directory\n\
         \x20 -h, --help               Show this help\n\
         \n\
         Config file (optional): $XDG_CONFIG_HOME/claude-code-usage-monitor/config.json\n\
         \x20 {{ \"claude\": true, \"codex\": false, \"window\": \"session\", \"language\": null }}"
    );
}
