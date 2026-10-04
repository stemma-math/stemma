//! `stemma upgrade`: moves a library to this version of `stemma`.
//!
//! It applies the migrations of every version in between, writes again the
//! files only `stemma` writes, from this version's templates, and updates the
//! dependencies. The result is one change, meant for one pull request.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

use crate::commands::{LEAN_TOOLCHAIN, STEMMA_GIT, mathlib_rev};
use crate::library::Library;
use crate::{agents_md, templates, ui};

/// A version, as `major.minor.patch`.
pub type Version = (u64, u64, u64);

/// Parses `major.minor.patch`.
pub fn parse_version(s: &str) -> Option<Version> {
    let mut parts = s.trim().split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// What this `stemma` is.
pub fn this_version() -> Version {
    parse_version(env!("CARGO_PKG_VERSION")).expect("the crate's version is major.minor.patch")
}

/// The files of a library a migration works on, in memory.
struct Files {
    config: String,
    agents: Option<String>,
}

/// A change of format, and how to move a library across it.
struct Migration {
    /// The version that introduced it.
    version: Version,
    description: &'static str,
    apply: fn(&mut Files, title: &str),
}

/// Every migration, oldest first.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: (0, 2, 0),
        description: "`self_merge` is now `merge_without_approval`",
        apply: |files, _| {
            files.config = files
                .config
                .lines()
                .map(|line| match line.trim_start().strip_prefix("self_merge") {
                    Some(rest) if rest.trim_start().starts_with('=') => {
                        line.replacen("self_merge", "merge_without_approval", 1)
                    }
                    _ => line.to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
        },
    },
    Migration {
        version: (0, 2, 0),
        description: "AGENTS.md belongs to the group, but for a block stemma keeps at its top",
        apply: |files, title| {
            // The AGENTS.md that 0.1.0 wrote, which held nothing of the group's.
            let old = format!(
                "# {title}\n\nThis is a Stemma library. Work on it through `stemma`: start agents with\n\
                 `stemma claude` or `stemma codex`, which give them the instructions, skills and\n\
                 permissions this library needs. An agent started directly is not equipped to\n\
                 work here.\n"
            );
            if files
                .agents
                .as_deref()
                .is_none_or(|text| text.trim() == old.trim())
            {
                files.agents = Some(agents_md::initial(title));
            }
        },
    },
];

/// The parts of a `lakefile.toml` that `stemma` writes from its choices.
#[derive(Deserialize)]
struct Lakefile {
    name: String,
    #[serde(default)]
    require: Vec<Require>,
}

#[derive(Deserialize)]
struct Require {
    name: String,
    path: Option<String>,
}

/// What `stemma upgrade` did, or would do.
struct Plan {
    from: String,
    migrations: Vec<&'static str>,
    /// Files whose content changes.
    changed: Vec<String>,
    writes: Vec<(String, String)>,
}

/// Works out the upgrade of a library, without writing anything.
fn plan(library: &Library) -> Result<Plan> {
    let dir = &library.dir;
    let config = &library.config.library;
    let from = parse_version(&config.stemma)
        .with_context(|| format!("stemma.toml names the version '{}'", config.stemma))?;
    let to = this_version();
    if from > to {
        bail!(
            "the library uses stemma {}, which is newer than this stemma ({}): install stemma {} instead",
            config.stemma,
            env!("CARGO_PKG_VERSION"),
            config.stemma
        );
    }
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
    let mut files = Files {
        config: read("stemma.toml").context("reading stemma.toml")?,
        agents: read("AGENTS.md"),
    };
    let mut migrations = Vec::new();
    for m in MIGRATIONS
        .iter()
        .filter(|m| m.version > from && m.version <= to)
    {
        (m.apply)(&mut files, &config.title);
        migrations.push(m.description);
    }
    // The version the library uses.
    files.config = files
        .config
        .lines()
        .map(|line| match line.trim_start().strip_prefix("stemma") {
            Some(rest) if rest.trim_start().starts_with('=') => {
                format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION"))
            }
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    // The files only `stemma` writes, from this version's templates.
    let lakefile: Lakefile =
        toml::from_str(&read("lakefile.toml").context("reading lakefile.toml")?)
            .context("reading lakefile.toml")?;
    let stemma_path = lakefile
        .require
        .iter()
        .find(|r| r.name == "stemma")
        .and_then(|r| r.path.clone());
    let mathlib = lakefile.require.iter().any(|r| r.name == "mathlib");
    let context = json!({
        "name": config.name,
        "title": config.title,
        "package": lakefile.name,
        "version": env!("CARGO_PKG_VERSION"),
        "stemma_git": STEMMA_GIT,
        "stemma_path": stemma_path,
        "mathlib": mathlib,
        "mathlib_rev": mathlib_rev(),
    });
    let agents = agents_md::updated(files.agents.as_deref().unwrap_or_default())
        .or(files.agents.clone())
        .unwrap_or_else(|| agents_md::initial(&config.title));
    let writes = vec![
        ("stemma.toml".to_string(), files.config),
        (
            "lakefile.toml".to_string(),
            templates::render("library/lakefile.toml", &context)?,
        ),
        ("lean-toolchain".to_string(), LEAN_TOOLCHAIN.to_string()),
        (
            ".github/workflows/stemma.yml".to_string(),
            templates::render("library/github/stemma.yml", &context)?,
        ),
        ("AGENTS.md".to_string(), agents),
    ];
    let changed = writes
        .iter()
        .filter(|(file, text)| read(file).as_deref() != Some(text.as_str()))
        .map(|(file, _)| file.clone())
        .collect();
    Ok(Plan {
        from: config.stemma.clone(),
        migrations,
        changed,
        writes,
    })
}

/// `stemma upgrade`.
pub fn upgrade(dry_run: bool, update: bool, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let plan = plan(&library)?;
    let to = env!("CARGO_PKG_VERSION");
    if !dry_run {
        for (file, text) in &plan.writes {
            let path = library.dir.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, text)?;
        }
    }
    let updated = if !dry_run && update && !plan.changed.is_empty() {
        let spinner = ui::Spinner::start("Updating the dependencies", json_output);
        let ok = Command::new("lake")
            .arg("update")
            .current_dir(&library.dir)
            .stdin(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success());
        spinner.stop();
        Some(ok)
    } else {
        None
    };
    if json_output {
        let value = json!({
            "from": plan.from, "to": to, "dry_run": dry_run,
            "migrations": plan.migrations, "changed": plan.changed, "updated": updated,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(updated != Some(false));
    }
    if plan.changed.is_empty() {
        ui::success(format!("The library already uses stemma {to}."));
        return Ok(true);
    }
    let verb = if dry_run { "Would upgrade" } else { "Upgraded" };
    ui::success(format!(
        "{verb} the library from stemma {} to {to}.",
        plan.from
    ));
    let (migrated, changed) = if dry_run {
        ("Would migrate", "Would change")
    } else {
        ("Migrated", "Changed")
    };
    for m in &plan.migrations {
        ui::note(format!("{migrated}: {m}."));
    }
    for f in &plan.changed {
        ui::note(format!("{changed} {f}."));
    }
    match updated {
        Some(true) => ui::success("Updated the dependencies."),
        Some(false) => ui::warning("Could not update the dependencies: run `lake update`."),
        None => {}
    }
    if !dry_run {
        ui::note(
            "Next: `stemma check`, then share the change as one pull request. New versions of \
             Lean or Mathlib can leave signatures stale: `stemma status` shows them.",
        );
    }
    Ok(updated != Some(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("0.2.1"), Some((0, 2, 1)));
        assert_eq!(parse_version("1.0"), None);
        assert_eq!(parse_version("0.2.1.4"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn renames_self_merge() {
        let mut files = Files {
            config: "[policy]\nself_merge = [\"signer\"]\nother = 1\n".into(),
            agents: None,
        };
        (MIGRATIONS[0].apply)(&mut files, "T");
        assert_eq!(
            files.config,
            "[policy]\nmerge_without_approval = [\"signer\"]\nother = 1\n"
        );
    }
}
