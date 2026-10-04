//! A Stemma library on disk: where it is, its modules and its table of contents.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::Config;

/// A library found on disk.
pub struct Library {
    /// The directory holding `stemma.toml`.
    pub dir: PathBuf,
    pub config: Config,
}

impl Library {
    /// Finds the library containing `start`, looking upwards for `stemma.toml`.
    pub fn find(start: &Path) -> Result<Self> {
        let mut dir = start
            .canonicalize()
            .with_context(|| format!("resolving {}", start.display()))?;
        loop {
            if dir.join("stemma.toml").is_file() {
                let config = Config::read(&dir)?;
                return Ok(Self { dir, config });
            }
            if !dir.pop() {
                bail!("not inside a Stemma library: no stemma.toml here or above");
            }
        }
    }

    /// The library's Lean name.
    pub fn name(&self) -> &str {
        &self.config.library.name
    }

    /// The path of the root module, the table of contents.
    pub fn root_path(&self) -> PathBuf {
        self.dir.join(format!("{}.lean", self.name()))
    }

    /// The path of a module of the library.
    pub fn module_path(&self, module: &str) -> PathBuf {
        let mut path = self.dir.clone();
        for part in module.split('.') {
            path.push(part);
        }
        path.set_extension("lean");
        path
    }

    /// Every module on disk under `<Library>/`, as Lean names, sorted.
    pub fn modules_on_disk(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let base = self.dir.join(self.name());
        if base.is_dir() {
            collect_modules(&base, self.name(), &mut out)?;
        }
        out.sort();
        Ok(out)
    }

    /// The modules the root module imports, in order.
    pub fn table_of_contents(&self) -> Result<Vec<String>> {
        let text = std::fs::read_to_string(self.root_path())
            .with_context(|| format!("reading {}", self.root_path().display()))?;
        Ok(parse_imports(&text)?
            .into_iter()
            .map(|(_, m)| m)
            .filter(|m| in_library(self.name(), m))
            .collect())
    }
}

fn collect_modules(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() {
            collect_modules(&path, &format!("{prefix}.{stem}"), out)?;
        } else if path.extension().is_some_and(|e| e == "lean") {
            out.push(format!("{prefix}.{stem}"));
        }
    }
    Ok(())
}

/// Whether `module` belongs to the library `name` (and is not its root).
pub fn in_library(name: &str, module: &str) -> bool {
    module
        .strip_prefix(name)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// The imports of a root module, with their line numbers, in order.
///
/// The root module has a fixed form, so that it is read exactly as Lean reads
/// it: it begins with its imports, one `import <Module>` per line (blank lines
/// between them are allowed), and its first other line is a command. Anything
/// else before that command (a comment, two modules on one line, `module`,
/// `public import`…) is an error, never a guess.
pub fn parse_imports(text: &str) -> Result<Vec<(usize, String)>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens[0] == "import" {
            if tokens.len() != 2 {
                bail!(
                    "line {}: the table of contents imports one module per line",
                    i + 1
                );
            }
            out.push((i, tokens[1].to_string()));
            continue;
        }
        // The first other line ends the imports, and must be a command.
        let header_word = ["module", "prelude", "public", "meta", "private"];
        if trimmed.starts_with("--")
            || trimmed.starts_with("/-")
            || header_word.contains(&tokens[0])
            || tokens.contains(&"import")
        {
            bail!(
                "line {}: the table of contents has nothing before its first command but \
                 `import` lines",
                i + 1
            );
        }
        break;
    }
    Ok(out)
}

/// Whether a string is a valid module name within the library `name`.
pub fn valid_module(name: &str, module: &str) -> bool {
    in_library(name, module)
        && module.split('.').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
        })
}

/// The parent of a module name: `A.B.C` has parent `A.B`.
fn parent(module: &str) -> &str {
    module.rsplit_once('.').map_or("", |(p, _)| p)
}

/// The number of leading name components two modules share.
fn shared_prefix(a: &str, b: &str) -> usize {
    a.split('.')
        .zip(b.split('.'))
        .take_while(|(x, y)| x == y)
        .count()
}

/// Inserts `import <module>` into the root module's text.
///
/// The import goes after `after` when given. Otherwise it goes after the last
/// import of the library module that shares the most with it (a sibling first),
/// or after the last import when there is none, so that the table of contents
/// keeps its structure without anyone thinking about it.
pub fn insert_import(
    text: &str,
    library: &str,
    module: &str,
    after: Option<&str>,
) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let parsed = parse_imports(text)?;
    let import_lines: Vec<(usize, &str)> = parsed.iter().map(|(i, m)| (*i, m.as_str())).collect();
    if import_lines.iter().any(|(_, m)| *m == module) {
        bail!("{module} is already in the table of contents");
    }
    let position = if let Some(after) = after {
        let Some((i, _)) = import_lines.iter().find(|(_, m)| *m == after) else {
            bail!("{after} is not in the table of contents");
        };
        *i
    } else {
        let best = import_lines
            .iter()
            .filter(|(_, m)| in_library(library, m))
            .map(|(i, m)| {
                let score = shared_prefix(parent(m), parent(module))
                    + usize::from(parent(m) == parent(module));
                (score, *i)
            })
            .max();
        match best {
            Some((_, i)) => i,
            None => match import_lines.last() {
                Some((i, _)) => *i,
                None => bail!("the root module has no imports"),
            },
        }
    };
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out.insert(position + 1, format!("import {module}"));
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "import Stemma\nimport Alg.Groups.Basic\nimport Alg.Rings.Basic\n\n#doc (Stemma) \"Alg\" =>\n";

    fn imports(text: &str) -> Vec<String> {
        parse_imports(text)
            .unwrap()
            .into_iter()
            .map(|(_, m)| m)
            .collect()
    }

    #[test]
    fn reads_imports() {
        assert_eq!(
            imports(ROOT),
            ["Stemma", "Alg.Groups.Basic", "Alg.Rings.Basic"]
        );
    }

    #[test]
    fn refuses_anything_else_before_the_first_command() {
        for text in [
            "import A B\n#doc",
            "-- a comment\nimport A\n#doc",
            "import A\n/- a comment -/\nimport B\n#doc",
            "module\nimport A\n#doc",
            "public import A\n#doc",
        ] {
            assert!(parse_imports(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn inserts_after_a_sibling() {
        let text = insert_import(ROOT, "Alg", "Alg.Groups.Lagrange", None).unwrap();
        assert_eq!(
            imports(&text),
            [
                "Stemma",
                "Alg.Groups.Basic",
                "Alg.Groups.Lagrange",
                "Alg.Rings.Basic"
            ]
        );
    }

    #[test]
    fn inserts_after_the_last_library_import() {
        let text = insert_import(ROOT, "Alg", "Alg.Fields", None).unwrap();
        assert_eq!(imports(&text).last().unwrap(), "Alg.Fields");
    }

    #[test]
    fn inserts_after_the_given_module() {
        let text = insert_import(ROOT, "Alg", "Alg.Intro", Some("Stemma")).unwrap();
        assert_eq!(imports(&text)[1], "Alg.Intro");
    }

    #[test]
    fn inserts_into_an_empty_table_of_contents() {
        let text = insert_import("import Stemma\n\n#doc", "Alg", "Alg.Basic", None).unwrap();
        assert_eq!(imports(&text), ["Stemma", "Alg.Basic"]);
        assert!(text.ends_with("#doc\n"));
    }

    #[test]
    fn refuses_a_module_already_listed() {
        assert!(insert_import(ROOT, "Alg", "Alg.Rings.Basic", None).is_err());
    }

    #[test]
    fn validates_module_names() {
        assert!(valid_module("Alg", "Alg.Groups.Basic"));
        assert!(!valid_module("Alg", "Alg"));
        assert!(!valid_module("Alg", "Other.Basic"));
        assert!(!valid_module("Alg", "Alg.2nd"));
        assert!(!valid_module("Alg", "Alg..Basic"));
    }
}
