//! How `stemma` talks to people: one style for every command.
//!
//! The roles, symbols and colour policy are stemma-math/design's
//! (`stemma-design-terminal`): success succeeded, danger failed, warning
//! awaits something, muted is detail. Colour follows `NO_COLOR`, whether the
//! output is a terminal and `TERM=dumb`, which also asks for ASCII symbols.
//! Output for agents (`--json`) never goes through here.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;
use std::time::Duration;

use indicatif::ProgressBar;
use stemma_design_terminal::{Output, Role, Stream, Symbol, themes};

/// The colour policy of standard output, decided once.
pub fn out() -> Output {
    static OUT: OnceLock<Output> = OnceLock::new();
    *OUT.get_or_init(|| Output::detect(Stream::Stdout))
}

/// The colour policy of standard error, where cliclack, the spinners and
/// errors write, decided once.
pub fn err() -> Output {
    static ERR: OnceLock<Output> = OnceLock::new();
    *ERR.get_or_init(|| Output::detect(Stream::Stderr))
}

/// Makes the libraries that draw for `stemma` follow the colour policy and
/// take the design's themes: clap's styles are set where the command is
/// built, and these are cliclack's and console's.
pub fn apply_themes() {
    out().apply_to_console(Stream::Stdout);
    err().apply_to_console(Stream::Stderr);
    cliclack::set_theme(themes::cliclack::Theme::new(err()));
}

/// `text` in `role`, for `output`.
pub fn paint(output: Output, role: Role, text: impl Display) -> String {
    output.paint(role, &text.to_string()).to_string()
}

/// A symbol in `role`, for standard output.
fn mark(symbol: Symbol, role: Role) -> String {
    let out = out();
    paint(out, role, out.symbol(symbol))
}

/// A headline, at the start of a command's output.
pub fn heading(text: impl Display) {
    println!("{}", paint(out(), Role::Strong, text));
}

/// Something that succeeded.
pub fn success(text: impl Display) {
    println!("{} {text}", mark(Symbol::Success, Role::Success));
}

/// Something that failed.
pub fn error(text: impl Display) {
    println!("{} {text}", mark(Symbol::Failure, Role::Danger));
}

/// Something that failed, on standard error: what stops a command.
pub fn error_stderr(text: impl Display) {
    let err = err();
    eprintln!(
        "{} {text}",
        paint(err, Role::Danger, err.symbol(Symbol::Failure))
    );
}

/// Something that awaits an action.
pub fn warning(text: impl Display) {
    println!("{} {text}", mark(Symbol::Warning, Role::Warning));
}

/// A detail, or what to do next.
pub fn note(text: impl Display) {
    println!(
        "{} {}",
        mark(Symbol::ArrowRight, Role::Info),
        paint(out(), Role::Muted, text)
    );
}

/// Muted text, for paths and details inside a line.
pub fn dim(text: impl Display) -> String {
    paint(out(), Role::Muted, text)
}

/// Strong text, for names inside a line.
pub fn bold(text: impl Display) -> String {
    paint(out(), Role::Strong, text)
}

/// The role of a state: an environment's, or its signature's.
fn state_role(state: &str) -> Option<Role> {
    match state {
        "proved" | "signed" => Some(Role::Success),
        "pending" | "unsigned" => Some(Role::Warning),
        "cited" => Some(Role::Info),
        "not formalized" | "stale" => Some(Role::Danger),
        _ => None,
    }
}

/// A state in its role, padded to `width` before painting so that columns align.
pub fn state(state: &str, width: usize) -> String {
    let padded = format!("{state:<width$}");
    match state_role(state) {
        Some(role) => paint(out(), role, padded),
        None => padded,
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
        let style = themes::indicatif::timed_spinner(&err());
        let bar = ProgressBar::new_spinner();
        bar.set_style(style);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_take_their_intents() {
        assert_eq!(state_role("signed"), Some(Role::Success));
        assert_eq!(state_role("unsigned"), Some(Role::Warning));
        assert_eq!(state_role("cited"), Some(Role::Info));
        assert_eq!(state_role("stale"), Some(Role::Danger));
        assert_eq!(state_role("other"), None);
        assert_eq!(paint(Output::plain(), Role::Danger, "stale"), "stale");
    }
}
