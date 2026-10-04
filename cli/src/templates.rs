//! The templates `stemma` writes files from, embedded in the binary.

use anyhow::{Context, Result};
use minijinja::Environment;
use serde::Serialize;

const TEMPLATES: &[(&str, &str)] = &[
    (
        "library/stemma.toml",
        include_str!("../templates/library/stemma.toml.jinja"),
    ),
    (
        "library/lakefile.toml",
        include_str!("../templates/library/lakefile.toml.jinja"),
    ),
    (
        "library/root.lean",
        include_str!("../templates/library/root.lean.jinja"),
    ),
    (
        "library/AGENTS.md",
        include_str!("../templates/library/AGENTS.md.jinja"),
    ),
    (
        "library/README.md",
        include_str!("../templates/library/README.md.jinja"),
    ),
    (
        "library/gitignore",
        include_str!("../templates/library/gitignore.jinja"),
    ),
    (
        "library/github/stemma.yml",
        include_str!("../templates/library/github/stemma.yml.jinja"),
    ),
    (
        "module/document.lean",
        include_str!("../templates/module/document.lean.jinja"),
    ),
    (
        "module/lean.lean",
        include_str!("../templates/module/lean.lean.jinja"),
    ),
    (
        "site/StemmaSite.lean",
        include_str!("../templates/site/StemmaSite.lean.jinja"),
    ),
];

/// Renders the template `name` with `context`.
pub fn render(name: &str, context: impl Serialize) -> Result<String> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    for (name, source) in TEMPLATES {
        env.add_template(name, source)?;
    }
    env.get_template(name)?
        .render(context)
        .with_context(|| format!("rendering the template {name}"))
}
