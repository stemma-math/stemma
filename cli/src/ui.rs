//! How `stemma` talks to people: one style for every command.
//!
//! Colors and symbols carry fixed meanings: green succeeded, red failed,
//! yellow awaits something, dim is detail. Everything respects `NO_COLOR` and
//! falls back to plain text when the output is not a terminal. Output for agents
//! (`--json`) never goes through here.

use std::fmt::Display;
use std::io::IsTerminal;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::{OwoColorize, Stream::Stdout};

/// A headline, at the start of a command's output.
pub fn heading(text: impl Display) {
    println!("{}", text.if_supports_color(Stdout, |t| t.bold()));
}

/// Something that succeeded.
pub fn success(text: impl Display) {
    println!("{} {text}", "✓".if_supports_color(Stdout, |t| t.green()));
}

/// Something that failed.
pub fn error(text: impl Display) {
    println!(
        "{} {text}",
        "✗".if_supports_color(Stdout, |t| t.red().bold().to_string())
    );
}

/// Something that awaits an action.
pub fn warning(text: impl Display) {
    println!("{} {text}", "!".if_supports_color(Stdout, |t| t.yellow()));
}

/// A detail, or what to do next.
pub fn note(text: impl Display) {
    println!(
        "{} {}",
        "→".if_supports_color(Stdout, |t| t.cyan()),
        text.if_supports_color(Stdout, |t| t.dimmed())
    );
}

/// Dimmed text, for paths and details inside a line.
pub fn dim(text: impl Display) -> String {
    text.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

/// Bold text, for names inside a line.
pub fn bold(text: impl Display) -> String {
    text.if_supports_color(Stdout, |t| t.bold()).to_string()
}

/// The color of a state, padded to `width` before coloring so that columns align.
pub fn state(state: &str, width: usize) -> String {
    let padded = format!("{state:<width$}");
    match state {
        "proved" | "signed" => padded.if_supports_color(Stdout, |t| t.green()).to_string(),
        "pending" | "unsigned" => padded.if_supports_color(Stdout, |t| t.yellow()).to_string(),
        "cited" => padded.if_supports_color(Stdout, |t| t.cyan()).to_string(),
        "not formalized" | "stale" => padded.if_supports_color(Stdout, |t| t.red()).to_string(),
        _ => padded,
    }
}

/// A spinner for a long step, shown only on a terminal.
pub struct Spinner(Option<ProgressBar>);

impl Spinner {
    /// Starts a spinner with a message, unless output is for agents or not a terminal.
    pub fn start(message: impl Into<String>, json_output: bool) -> Self {
        if json_output || !std::io::stderr().is_terminal() {
            return Self(None);
        }
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg} {elapsed:.dim}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        bar.set_message(message.into());
        bar.enable_steady_tick(Duration::from_millis(80));
        Self(Some(bar))
    }

    /// Stops the spinner, leaving nothing behind.
    pub fn stop(self) {
        if let Some(bar) = self.0 {
            bar.finish_and_clear();
        }
    }
}
