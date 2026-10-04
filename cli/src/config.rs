//! The library's configuration, `stemma.toml`.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// The contents of `stemma.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub library: Library,
}

/// The `[library]` table.
#[derive(Debug, Deserialize)]
pub struct Library {
    /// The library's Lean name, such as `Algebra`.
    pub name: String,
    /// The site's title.
    pub title: String,
    /// The `stemma` version the library uses.
    pub stemma: String,
}

impl Config {
    /// Reads `stemma.toml` from a library's directory.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join("stemma.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}
