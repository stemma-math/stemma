//! How `stemma --help` looks: commands in sections, with one accent color.

use std::fmt::Write;

use clap::Command;
use clap::builder::StyledStr;
use clap::builder::styling::{AnsiColor, Effects, Styles};

/// The commands, by what they are for, in the order the help shows them.
pub const SECTIONS: &[(&str, &[&str])] = &[
    ("Start", &["init", "new"]),
    ("Work with an agent", &["claude", "codex"]),
    (
        "See where you are",
        &["status", "check", "preview", "readback"],
    ),
    ("Share and sign", &["share", "sign", "key"]),
    ("Maintain", &["upgrade", "verify"]),
];

/// The style of every help page: bold headings, command names in one accent
/// color, placeholders dimmed. Colors follow the terminal and `NO_COLOR`.
pub fn styles() -> Styles {
    Styles::styled()
        .header(Effects::BOLD.into())
        .usage(Effects::BOLD.into())
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::BrightBlack.on_default())
        .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Yellow.on_default())
}

/// The template of the top-level help: a title, the usage, the commands in
/// sections, and the options.
pub fn template(command: &Command) -> StyledStr {
    let styles = styles();
    let (header, literal, dim) = (
        styles.get_header(),
        styles.get_literal(),
        styles.get_placeholder(),
    );
    let width = SECTIONS
        .iter()
        .flat_map(|(_, names)| names.iter())
        .map(|n| n.len())
        .max()
        .unwrap_or(0)
        + 2;
    let mut s = StyledStr::new();
    let version = command.get_version().unwrap_or_default();
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    let _ = writeln!(
        s,
        "{literal}stemma{literal:#} {dim}{version}{dim:#}  {about}\n"
    );
    let _ = writeln!(s, "{{usage-heading}} {{usage}}");
    for (title, names) in SECTIONS {
        let _ = writeln!(s, "\n{header}{title}{header:#}");
        for name in *names {
            let about = command
                .find_subcommand(name)
                .and_then(Command::get_about)
                .map(ToString::to_string)
                .unwrap_or_default();
            let _ = writeln!(s, "  {literal}{name:<width$}{literal:#}{about}");
        }
    }
    let _ = write!(
        s,
        "\n{header}Options{header:#}\n{{options}}\n\n{dim}Run{dim:#} {literal}stemma <command> --help{literal:#} {dim}for a command's options.{dim:#}"
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn every_command_is_in_one_section() {
        let command = crate::Cli::command();
        let listed: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|(_, n)| n.iter().copied())
            .collect();
        for sub in command.get_subcommands() {
            let name = sub.get_name();
            if sub.is_hide_set() || name == "help" {
                continue;
            }
            assert_eq!(
                listed.iter().filter(|n| **n == name).count(),
                1,
                "`{name}` must be in exactly one section of the help"
            );
        }
        for name in &listed {
            assert!(
                command.find_subcommand(name).is_some(),
                "`{name}` is not a command"
            );
        }
    }
}
