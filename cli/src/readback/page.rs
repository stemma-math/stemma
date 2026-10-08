//! The read-back page: one self-contained HTML file with an index, grouping
//! by module, filters, and the person's marks and notes.

use anyhow::{Context, Result};
use minijinja::Environment;
use serde::Serialize;

use super::reviews::{Mark, Reviews};
use super::{Entry, State};

/// The page's template. Its name ends in `.html`, so that every value is
/// escaped for HTML.
const TEMPLATE: &str = include_str!("page.html");

/// What the page shows of one read-back.
#[derive(Serialize)]
struct Item<'a> {
    label: &'a str,
    display: &'a str,
    module: &'a str,
    prose: &'a str,
    lean: String,
    readback: &'a str,
    agent: &'a str,
    state: State,
    /// Whether it was made of other Lean than the environment's, archived or not.
    stale: bool,
    /// Whether the environment is gone from the library, or has no Lean.
    gone: bool,
    mark: Mark,
    note: &'a str,
    /// Whether the mark and the note were made on an earlier read-back.
    outdated: bool,
    search: String,
}

/// The Lean of an environment, without the fences of its blocks.
fn code(lean: &str) -> String {
    lean.lines()
        .filter(|l| !matches!(l.trim(), "```" | "```lean"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_string()
}

/// The read-backs of one module.
#[derive(Serialize)]
struct Group<'a> {
    module: &'a str,
    items: Vec<Item<'a>>,
}

/// Renders the page. With a token, it is the page the local server serves,
/// which can write marks; without one, it is a copy that only shows them.
pub(super) fn render(
    title: &str,
    entries: &[Entry],
    reviews: &Reviews,
    token: Option<&str>,
) -> Result<String> {
    let mut groups: Vec<Group> = Vec::new();
    for e in entries {
        let r = &e.readback;
        let review = reviews.readbacks.get(&r.label);
        let item = Item {
            label: &r.label,
            display: &r.display,
            module: &r.module,
            prose: &r.prose,
            lean: code(&r.lean),
            readback: &r.readback,
            agent: &r.agent,
            state: e.state(reviews),
            stale: e.freshness == State::Stale,
            gone: e.gone,
            mark: review.map_or(Mark::Unread, |v| v.mark),
            note: review.map_or("", |v| v.note.as_str()),
            outdated: review.is_some_and(|v| v.outdated(&r.formal)),
            search: format!("{} {} {}", r.label, r.display, r.module).to_lowercase(),
        };
        match groups.last_mut() {
            Some(g) if g.module == r.module => g.items.push(item),
            _ => groups.push(Group {
                module: &r.module,
                items: vec![item],
            }),
        }
    }
    let mut env = Environment::new();
    env.add_template("page.html", TEMPLATE)?;
    env.get_template("page.html")?
        .render(minijinja::context! {
            title => title,
            token => token,
            groups => groups,
        })
        .context("rendering the read-back page")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_lean_is_shown_without_fences() {
        let lean = "```lean\ndef A := 1\n```\n\n```lean\ndef B := 2\n```";
        assert_eq!(super::code(lean), "def A := 1\n\ndef B := 2");
    }
}
