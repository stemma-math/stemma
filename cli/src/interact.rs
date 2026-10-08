//! When a command asks a person for decisions, and how it asks.
//!
//! A command asks questions only when all of these hold:
//!
//! - stdin and stdout are terminals;
//! - it is not running in an agent's session ([`SESSION_VARIABLE`]);
//! - there is no `--json`;
//! - there is no `--yes`.
//!
//! Every question has an equivalent flag, and `--yes` accepts what the command
//! would do. Without interaction, a consequential decision is never guessed:
//! when it has no safe default and no flag gives it, the command fails with
//! [`needs_flag`], which names the flag to pass.
//!
//! Questions are asked with `cliclack`, through the helpers here, so that every
//! command looks the same and a cancelled question (Esc, Ctrl-C) reads the same.

use std::fmt::Display;
use std::io::IsTerminal;

use anyhow::{Result, anyhow};

/// The environment variable that marks a process as part of an agent's session.
pub const SESSION_VARIABLE: &str = "STEMMA_SESSION";

/// Whether this process runs in an agent's session.
pub fn in_agent_session() -> bool {
    std::env::var_os(SESSION_VARIABLE).is_some()
}

/// How a command may decide: by asking a person, or from its flags alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    interactive: bool,
    agent_session: bool,
    yes: bool,
}

impl Mode {
    /// The mode of this process, for a command run with `--json` and `--yes`
    /// as given.
    pub fn detect(json: bool, yes: bool) -> Self {
        let agent_session = in_agent_session();
        let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        Self {
            interactive: terminal && !agent_session && !json && !yes,
            agent_session,
            yes,
        }
    }

    /// Whether the command may ask questions.
    pub fn interactive(self) -> bool {
        self.interactive
    }

    /// Whether the command runs in an agent's session. Some decisions (those
    /// that force a push or discard work) are a person's, and are never taken
    /// there, whatever the flags.
    pub fn agent_session(self) -> bool {
        self.agent_session
    }

    /// Whether `--yes` was given: accept what the command would do.
    pub fn yes(self) -> bool {
        self.yes
    }
}

/// The failure of a command that needs a decision it may not guess: it names
/// the flag that gives it.
pub fn needs_flag(decision: impl Display, flag: &str) -> anyhow::Error {
    let how = if in_agent_session() {
        "stemma asks no questions in an agent's session"
    } else {
        "stemma asks questions only in an interactive terminal, without --json or --yes"
    };
    anyhow!("{decision}: pass {flag} ({how})")
}

/// Turns a cancelled question into a plain error.
fn answered<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|error| {
        if error.kind() == std::io::ErrorKind::Interrupted {
            anyhow!("cancelled")
        } else {
            error.into()
        }
    })
}

/// Opens an interactive command: `stemma <command>`.
pub fn intro(command: &str) -> Result<()> {
    answered(cliclack::intro(crate::ui::bold(format!(
        " stemma {command} "
    ))))
}

/// Closes an interactive command.
pub fn outro(message: impl Display) -> Result<()> {
    answered(cliclack::outro(message))
}

/// Closes an interactive command that did not finish what it set out to do.
pub fn outro_cancel(message: impl Display) -> Result<()> {
    answered(cliclack::outro_cancel(message))
}

/// A yes-or-no question.
pub fn confirm(prompt: impl Display, default: bool) -> Result<bool> {
    answered(cliclack::confirm(prompt).initial_value(default).interact())
}

/// A choice among `items`, as (value, label, hint), starting at `initial`.
pub fn select<T: Clone + Eq>(
    prompt: impl Display,
    items: &[(T, String, String)],
    initial: Option<T>,
) -> Result<T> {
    let mut select = cliclack::select(prompt).items(items);
    if let Some(initial) = initial {
        select = select.initial_value(initial);
    }
    answered(select.interact())
}

/// Any number of `items`, as (value, label, hint); none is chosen at first.
pub fn multiselect<T: Clone + Eq>(
    prompt: impl Display,
    items: &[(T, String, String)],
) -> Result<Vec<T>> {
    answered(
        cliclack::multiselect(prompt)
            .items(items)
            .required(false)
            .interact(),
    )
}

/// A line of text, starting from `default`, checked by `valid` (which says
/// what is wrong).
pub fn text(
    prompt: impl Display,
    default: &str,
    valid: impl Fn(&str) -> Option<&'static str> + 'static,
) -> Result<String> {
    let mut input = cliclack::input(prompt);
    if !default.is_empty() {
        input = input.default_input(default);
    }
    answered(
        input
            .validate(move |s: &String| valid(s).map_or(Ok(()), Err))
            .interact(),
    )
}

/// Several lines of text, starting from `default`; Esc submits them.
pub fn paragraph(prompt: impl Display, default: &str) -> Result<String> {
    let mut input = cliclack::input(prompt).multiline().required(false);
    if !default.is_empty() {
        input = input.default_input(default);
    }
    answered(input.interact())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_yes_are_never_interactive() {
        assert!(!Mode::detect(true, false).interactive());
        assert!(!Mode::detect(false, true).interactive());
        assert!(Mode::detect(false, true).yes());
    }

    #[test]
    fn the_failure_names_the_flag() {
        let error = needs_flag("which branch to work on", "--branch <name>");
        assert!(error.to_string().contains("pass --branch <name>"));
    }
}
