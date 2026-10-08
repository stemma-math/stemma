//! How `stemma sign` shows an environment awaiting a signature: a compact
//! card with its name, label and state, its prose, its Lean statement, where
//! it is, and a link to its read-back when there is one.
//!
//! Cards are drawn inside cliclack's frame, on standard error, with the
//! terminal's own colors (so they read in dark and light themes alike), and
//! plainly under `NO_COLOR` or when colors are not supported.

use console::Style;

/// The most lines a card shows of prose and Lean together; beyond it, the
/// card shows only where to read them.
const MAX_LINES: usize = 16;

/// The widest a card's text grows, even on a wide terminal.
const MAX_WIDTH: usize = 96;

/// What a card shows.
pub struct Card<'a> {
    /// The environment's name, such as "Theorem".
    pub display: &'a str,
    pub label: &'a str,
    /// Why it awaits a signature: `None` when it is new, else what changed.
    pub stale: Option<&'a str>,
    pub prose: &'a str,
    /// Its Lean, as written in the document.
    pub lean: &'a str,
    /// Its base kind: a definition's values are part of what is signed, a
    /// statement's proofs are not.
    pub base: &'a str,
    /// Where it is: `Alg/Even.lean:12-20`.
    pub location: String,
    /// Who last touched it, for what others left pending.
    pub author: Option<String>,
    /// Its read-back, when one is current.
    pub readback: Option<String>,
}

fn styled(style: Style) -> Style {
    style.for_stderr()
}

/// Bold text, on standard error.
pub fn bold(text: impl std::fmt::Display) -> String {
    styled(Style::new().bold()).apply_to(text).to_string()
}

/// Dimmed text, on standard error.
pub fn dim(text: impl std::fmt::Display) -> String {
    styled(Style::new().dim()).apply_to(text).to_string()
}

/// The state of an item, colored as everywhere else: yellow awaits, red is
/// stale.
pub fn state(stale: bool, text: impl std::fmt::Display) -> String {
    let style = if stale {
        Style::new().red()
    } else {
        Style::new().yellow()
    };
    styled(style).apply_to(text).to_string()
}

/// The width cards are wrapped to: the terminal's, less the frame.
fn width() -> usize {
    let (_, columns) = console::Term::stderr().size();
    (columns as usize).saturating_sub(6).clamp(40, MAX_WIDTH)
}

/// Text wrapped to the width of cliclack's frame.
pub fn fill(text: &str) -> String {
    textwrap::fill(text, width())
}

/// The Lean worth reading before signing: no code fences; for a statement,
/// each declaration up to its proof, which no signature covers.
pub fn statement(lean: &str, base: &str) -> String {
    let lines: Vec<&str> = lean
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect();
    if base != "statement" {
        return lines.join("\n").trim().to_string();
    }
    // Declarations start at the margin; what is indented continues them.
    let mut declarations: Vec<String> = Vec::new();
    for line in lines {
        let starts = !line.is_empty() && !line.starts_with(char::is_whitespace);
        match declarations.last_mut() {
            Some(last) if !starts => {
                last.push('\n');
                last.push_str(line);
            }
            _ => declarations.push(line.to_string()),
        }
    }
    declarations
        .iter()
        .map(|d| match d.find(":=") {
            Some(at) => d[..at].trim_end().to_string(),
            None => d.trim_end().to_string(),
        })
        .filter(|d| !d.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prose without the blank lines a document puts around its blocks.
fn prose(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for paragraph in text.trim().split("\n\n") {
        let joined = paragraph
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            out.push(joined);
        }
    }
    out
}

impl Card<'_> {
    /// The card's lines, without the frame.
    pub fn render(&self, label_width: usize) -> String {
        self.render_at(label_width, width())
    }

    fn render_at(&self, label_width: usize, width: usize) -> String {
        let kind_width = 12;
        let status = match self.stale {
            None => state(false, "new"),
            Some(why) => state(true, format!("stale: {why}")),
        };
        let mut out = vec![format!(
            "{}{} {}  {status}",
            dim(format!("{:<kind_width$}", self.display)),
            bold(self.label),
            " ".repeat(label_width.saturating_sub(self.label.chars().count())),
        )];
        let indent = "  ";
        let text_width = width.saturating_sub(indent.len()).max(20);
        let mut body: Vec<String> = Vec::new();
        for paragraph in prose(self.prose) {
            if !body.is_empty() {
                body.push(String::new());
            }
            for line in textwrap::wrap(&paragraph, text_width) {
                body.push(format!("{indent}{line}"));
            }
        }
        let lean = statement(self.lean, self.base);
        if !lean.is_empty() {
            if !body.is_empty() {
                body.push(String::new());
            }
            let code = styled(Style::new().cyan());
            for line in lean.lines() {
                // Lean is not wrapped: its layout is part of how it reads.
                body.push(format!("{indent}{}", code.apply_to(line)));
            }
        }
        if body.len() > MAX_LINES {
            let where_ = if self.readback.is_some() {
                "Too long to show here: read it in its read-back, or where it is."
            } else {
                "Too long to show here: read it where it is."
            };
            out.push(format!("{indent}{}", dim(where_)));
        } else {
            out.extend(body);
        }
        let mut details = vec![self.location.clone()];
        if let Some(author) = &self.author {
            details.push(format!("last changed by {author}"));
        }
        for line in textwrap::wrap(&details.join(" · "), text_width) {
            out.push(format!("{indent}{}", dim(line)));
        }
        match &self.readback {
            Some(link) => out.push(format!("{indent}{} {link}", dim("read-back"))),
            None => out.push(format!(
                "{indent}{}",
                dim(format!("no read-back: `stemma readback {}`", self.label))
            )),
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statement_shows_without_its_proof() {
        let lean = "```lean\ntheorem IsEven.add {m n : Nat} (hm : IsEven m)\n    (hn : IsEven n) : \
                    IsEven (m + n) := by\n  obtain ⟨a, rfl⟩ := hm\n  exact ⟨_, rfl⟩\n\n\
                    theorem two : IsEven 2 := ⟨1, rfl⟩\n```";
        assert_eq!(
            statement(lean, "statement"),
            "theorem IsEven.add {m n : Nat} (hm : IsEven m)\n    (hn : IsEven n) : IsEven (m + n)\n\
             theorem two : IsEven 2"
        );
    }

    #[test]
    fn a_definition_shows_whole() {
        let lean = "```lean\ndef IsEven (n : Nat) : Prop := ∃ k, n = 2 * k\n```";
        assert_eq!(
            statement(lean, "definition"),
            "def IsEven (n : Nat) : Prop := ∃ k, n = 2 * k"
        );
    }

    fn card<'a>(prose: &'a str, lean: &'a str) -> Card<'a> {
        Card {
            display: "Theorem",
            label: "even-add",
            stale: None,
            prose,
            lean,
            base: "statement",
            location: "Alg/Even.lean:3-9".into(),
            author: Some("bob (claude-opus-5-5)".into()),
            readback: None,
        }
    }

    #[test]
    fn cards_are_compact_and_wrapped() {
        let text = card(
            "The sum of two even numbers\nis even, and this sentence is long enough to wrap.",
            "theorem t : True := trivial",
        )
        .render_at(10, 40);
        // Whether colors are on depends on where the tests run.
        let text = console::strip_ansi_codes(&text).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("Theorem     even-add"), "{text}");
        assert!(lines[0].ends_with("new"), "{text}");
        // Everything wraps but the read-back's link, the last line.
        let wrapped = &lines[..lines.len() - 1];
        assert!(wrapped.iter().all(|l| l.chars().count() <= 42), "{text}");
        assert!(text.contains("  theorem t : True\n"), "{text}");
        assert!(text.contains("bob (claude-opus-5-5)"), "{text}");
        assert!(text.contains("stemma readback even-add"));
    }

    #[test]
    fn long_items_show_only_where_to_read_them() {
        let prose = "A paragraph.\n\n".repeat(20);
        let text = card(&prose, "theorem t : True := trivial").render_at(8, 80);
        let text = console::strip_ansi_codes(&text).into_owned();
        assert!(text.contains("Too long to show here"), "{text}");
        assert!(!text.contains("A paragraph."), "{text}");
        assert!(text.contains("Alg/Even.lean:3-9"));
    }
}
