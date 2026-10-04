//! `AGENTS.md`: the group's own instructions for agents, with one block that
//! `stemma` writes.
//!
//! The block lets any agent started without `stemma` notice it: an equipped
//! agent's instructions contain the section the block names, and an agent
//! whose instructions do not must tell the person before doing anything.

use std::path::Path;

use anyhow::Result;

const BEGIN: &str = "<!-- stemma: begin (written by stemma; do not edit) -->";
const END: &str = "<!-- stemma: end -->";

/// The block `stemma` keeps in `AGENTS.md`.
pub fn block() -> String {
    format!(
        "{BEGIN}\n\
IMPORTANT. This is a Stemma library. Before your first answer or action, check\n\
that your instructions include a section titled \"Working in a Stemma library\".\n\
If they do not, you were not started through `stemma`: do not start the task.\n\
Tell the person, and ask them to start you again with `stemma claude` or\n\
`stemma codex`.\n\
{END}"
    )
}

/// The text of a new `AGENTS.md`: the block, and room for the group.
pub fn initial(title: &str) -> String {
    format!(
        "{}\n\n# {title}\n\n\
Instructions of this group for agents go here: conventions, what to record,\n\
how to work together.\n",
        block()
    )
}

/// `text` with the block present and current, or `None` when it already is.
pub fn updated(text: &str) -> Option<String> {
    let block = block();
    let new = match (text.find(BEGIN), text.find(END)) {
        (Some(start), Some(end)) if start < end => {
            let end = end + END.len();
            if text[start..end] == block {
                return None;
            }
            format!("{}{block}{}", &text[..start], &text[end..])
        }
        _ => format!("{block}\n\n{text}"),
    };
    Some(new)
}

/// Whether the library's `AGENTS.md` has the current block.
pub fn is_current(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("AGENTS.md")).is_ok_and(|t| updated(&t).is_none())
}

/// Writes the current block into the library's `AGENTS.md`, creating it if needed.
/// Returns whether anything changed.
pub fn ensure(dir: &Path, title: &str) -> Result<bool> {
    let path = dir.join("AGENTS.md");
    let new = match std::fs::read_to_string(&path) {
        Ok(text) => match updated(&text) {
            Some(new) => new,
            None => return Ok(false),
        },
        Err(_) => initial(title),
    };
    std::fs::write(path, new)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_has_the_block_first() {
        let text = initial("Algebra");
        assert!(text.starts_with(BEGIN));
        assert!(updated(&text).is_none());
    }

    #[test]
    fn the_group_text_is_kept() {
        let text = updated("# Our rules\n\nOpen friction issues.\n").unwrap();
        assert!(text.starts_with(BEGIN));
        assert!(text.ends_with("Open friction issues.\n"));
    }

    #[test]
    fn an_old_block_is_replaced_in_place() {
        let old = format!("# Rules\n\n{BEGIN}\nold text\n{END}\n\nMore rules.\n");
        let new = updated(&old).unwrap();
        assert!(new.starts_with("# Rules\n\n"));
        assert!(new.contains("Working in a Stemma library"));
        assert!(new.ends_with("\n\nMore rules.\n"));
        assert!(updated(&new).is_none());
    }
}
