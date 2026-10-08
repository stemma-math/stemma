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
use toml_edit::{Array, DocumentMut, Item, value};

use crate::commands::{LEAN_TOOLCHAIN, STEMMA_GIT, mathlib_rev};
use crate::config::Config;
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

/// The files of a library a migration works on, in memory. `stemma.toml` is
/// edited through its structure, never through its text.
struct Files {
    config: DocumentMut,
    agents: Option<String>,
}

/// A change of format, and how to move a library across it.
struct Migration {
    /// The version that introduced it.
    version: Version,
    description: &'static str,
    apply: fn(&mut Files, title: &str),
}

/// Renames `table.from` to `table.to`, keeping its value.
fn rename_key(doc: &mut DocumentMut, table: &str, from: &str, to: &str) {
    if let Some(t) = doc.get_mut(table).and_then(Item::as_table_like_mut)
        && let Some(v) = t.remove(from)
    {
        t.insert(to, v);
    }
}

/// Removes `table.key`, and the table when nothing is left in it.
fn remove_key(doc: &mut DocumentMut, table: &str, key: &str) {
    let Some(t) = doc.get_mut(table).and_then(Item::as_table_like_mut) else {
        return;
    };
    t.remove(key);
    if t.is_empty() {
        doc.remove(table);
    }
}

/// Gives the member `name` the key `key`, when that member has no keys (the
/// field is missing, or empty).
pub fn add_member_key(doc: &mut DocumentMut, name: &str, key: &str) {
    let Some(member) = doc
        .get_mut("members")
        .and_then(Item::as_table_like_mut)
        .and_then(|m| m.get_mut(name))
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };
    let empty = member
        .get("keys")
        .is_none_or(|k| k.as_array().is_some_and(Array::is_empty));
    if empty {
        member.insert("keys", value(Array::from_iter([key])));
    }
}

/// Every migration, oldest first.
///
/// A migration is marked with the version that introduces it, which may be the
/// next one while it is being developed: `Cargo.toml` changes only when that
/// version is released. Every migration must be idempotent, since a library
/// upgraded with a development build gets it again with the release.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: (0, 2, 0),
        description: "`self_merge` is now `merge_without_approval`",
        apply: |files, _| {
            rename_key(
                &mut files.config,
                "policy",
                "self_merge",
                "merge_without_approval",
            )
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
    Migration {
        version: (0, 3, 0),
        description: "approvals are signed commits, checked against members' keys: \
            `merge_without_approval` and `distinct_from_author` are gone, and members \
            need `keys`",
        apply: |files, _| {
            remove_key(&mut files.config, "policy", "merge_without_approval");
            remove_key(&mut files.config, "signatures", "distinct_from_author");
        },
    },
    Migration {
        version: (0, 4, 0),
        description: "central environments must be signed: `[policy] require_signed_central \
            = true`. Central environments already on main without a current signature now \
            make `stemma check` fail until a signer signs them with `stemma sign`; \
            `stemma status` lists them. A group that does not want this sets it to false, in \
            a change a maintainer approves",
        apply: |files, _| require_signed_central(&mut files.config),
    },
];

/// Writes `[policy] require_signed_central = true`, unless the library says
/// already what it wants.
fn require_signed_central(doc: &mut DocumentMut) {
    let policy = doc
        .entry("policy")
        .or_insert_with(|| Item::Table(toml_edit::Table::new()));
    if let Some(table) = policy.as_table_mut() {
        table.set_implicit(false);
    }
    if let Some(table) = policy.as_table_like_mut()
        && table.get("require_signed_central").is_none()
    {
        table.insert("require_signed_central", value(true));
    }
}

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
        config: read("stemma.toml")
            .context("reading stemma.toml")?
            .parse::<DocumentMut>()
            .context("reading stemma.toml")?,
        agents: read("AGENTS.md"),
    };
    let mut migrations = Vec::new();
    // Every migration newer than the library, including those of the version
    // being developed (see `MIGRATIONS`).
    for m in MIGRATIONS.iter().filter(|m| m.version > from) {
        (m.apply)(&mut files, &config.title);
        migrations.push(m.description);
    }
    // The person upgrading signs with their key: when their member has none,
    // it is added, so that they can sign and approve with this version.
    if let Some(key) = crate::keys::signing_key(dir)
        && Config::parse(&files.config.to_string()).is_ok_and(|c| c.member_with_key(&key).is_none())
    {
        add_member_key(&mut files.config, &crate::agent::person(), &key);
    }
    // The version the library uses.
    files.config["library"]["stemma"] = value(env!("CARGO_PKG_VERSION"));
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
        ("stemma.toml".to_string(), files.config.to_string()),
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

/// Runs git in the library, failing with its error.
fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    anyhow::ensure!(
        out.status.success(),
        "`git {}` failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// `stemma upgrade`. On `main`, the upgrade goes to a branch of its own; it is
/// committed unless `commit` is false, ready to share.
pub fn upgrade(dry_run: bool, update: bool, commit: bool, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let plan = plan(&library)?;
    let to = env!("CARGO_PKG_VERSION");
    let dir = &library.dir;
    let in_git = crate::branches::current(dir).is_some();
    let mut branch = crate::branches::current(dir);
    if !dry_run && !plan.changed.is_empty() && branch.as_deref() == Some("main") {
        branch = Some(crate::branches::move_to_new(
            dir,
            &format!("upgrade/stemma-{to}"),
        )?);
    }
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
    let committed = if !dry_run && commit && in_git && !plan.changed.is_empty() {
        let mut files: Vec<&str> = plan.changed.iter().map(String::as_str).collect();
        if dir.join("lake-manifest.json").exists() {
            files.push("lake-manifest.json");
        }
        let mut add = vec!["add", "--"];
        add.extend(&files);
        git(dir, &add)?;
        let message = format!("Upgrade to stemma {to}");
        let mut commit_args = vec!["commit", "--quiet", "-m", &message, "--"];
        commit_args.extend(&files);
        git(dir, &commit_args)?;
        true
    } else {
        false
    };
    if json_output {
        let value = json!({
            "from": plan.from, "to": to, "dry_run": dry_run,
            "migrations": plan.migrations, "changed": plan.changed, "updated": updated,
            "branch": branch, "committed": committed,
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
    if committed {
        ui::success(format!(
            "Committed it on the branch {}.",
            ui::bold(branch.as_deref().unwrap_or_default())
        ));
        ui::note(
            "Next: `stemma check`, then `stemma share`. New versions of Lean or Mathlib can \
             leave signatures stale: `stemma status` shows them.",
        );
    } else if !dry_run {
        ui::note(
            "Next: `stemma check`, commit, and share the change as one pull request. New \
             versions of Lean or Mathlib can leave signatures stale: `stemma status` shows them.",
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

    fn doc(text: &str) -> DocumentMut {
        text.parse().unwrap()
    }

    #[test]
    fn adds_a_key_to_a_member_without_one() {
        let mut d = doc(
            "[members]\nalice = { roles = [\"maintainer\"] }\nbob = { roles = [] }\n\
             dan = { roles = [\"signer\"], keys = [] }\n",
        );
        add_member_key(&mut d, "alice", "ssh-ed25519 AAAA");
        add_member_key(&mut d, "dan", "ssh-ed25519 DDDD");
        add_member_key(&mut d, "carol", "ssh-ed25519 CCCC");
        let c: toml::Value = toml::from_str(&d.to_string()).unwrap();
        assert_eq!(
            c["members"]["alice"]["keys"][0].as_str(),
            Some("ssh-ed25519 AAAA")
        );
        assert!(c["members"]["bob"].get("keys").is_none());
        assert_eq!(
            c["members"]["dan"]["keys"][0].as_str(),
            Some("ssh-ed25519 DDDD")
        );
        assert!(c["members"].get("carol").is_none());
    }

    #[test]
    fn a_key_in_another_table_is_left_alone() {
        // A member named like a key of [library] is a member, not that key.
        let mut d = doc("[library]\nstemma = \"0.1.0\"\n\n[members]\nstemma = { roles = [] }\n");
        d["library"]["stemma"] = value("0.3.0");
        let c: toml::Value = toml::from_str(&d.to_string()).unwrap();
        assert_eq!(c["library"]["stemma"].as_str(), Some("0.3.0"));
        assert!(c["members"]["stemma"].is_table());
    }

    #[test]
    fn migrations_are_idempotent() {
        let config = "[library]\nname = \"T\"\ntitle = \"T\"\nstemma = \"0.1.0\"\n\n\
            [members]\nalice = { roles = [\"maintainer\"] }\n\n\
            [policy]\nself_merge = [\"signer\"]\n\n[signatures]\ndistinct_from_author = true\n";
        for m in MIGRATIONS {
            let mut once = Files {
                config: doc(config),
                agents: None,
            };
            (m.apply)(&mut once, "T");
            let mut twice = Files {
                config: once.config.clone(),
                agents: once.agents.clone(),
            };
            (m.apply)(&mut twice, "T");
            assert_eq!(
                twice.config.to_string(),
                once.config.to_string(),
                "{}",
                m.description
            );
            assert_eq!(twice.agents, once.agents, "{}", m.description);
        }
    }

    #[test]
    fn signed_central_environments_become_required_explicitly() {
        for (before, expected) in [
            ("[library]\nname = \"T\"\n", true),
            ("[policy.review]\npolicy = [\"signer\"]\n", true),
            ("[policy]\nrequire_signed_central = false\n", false),
        ] {
            let mut d = doc(before);
            require_signed_central(&mut d);
            let c: toml::Value = toml::from_str(&d.to_string()).unwrap();
            assert_eq!(
                c["policy"]["require_signed_central"].as_bool(),
                Some(expected),
                "{d}"
            );
        }
        let mut d = doc("[policy.review]\npolicy = [\"signer\"]\n");
        require_signed_central(&mut d);
        let c: toml::Value = toml::from_str(&d.to_string()).unwrap();
        assert_eq!(c["policy"]["review"]["policy"][0].as_str(), Some("signer"));
    }

    #[test]
    fn migrations_are_in_order() {
        assert!(MIGRATIONS.windows(2).all(|w| w[0].version <= w[1].version));
    }

    #[test]
    fn renames_and_removes_policy_keys() {
        let mut files = Files {
            config: doc(
                "[policy]\nself_merge = [\"signer\"]\nother = 1\n\n[signatures]\ndistinct_from_author = true\n",
            ),
            agents: None,
        };
        (MIGRATIONS[0].apply)(&mut files, "T");
        let c: toml::Value = toml::from_str(&files.config.to_string()).unwrap();
        assert_eq!(
            c["policy"]["merge_without_approval"][0].as_str(),
            Some("signer")
        );
        (MIGRATIONS[2].apply)(&mut files, "T");
        let c: toml::Value = toml::from_str(&files.config.to_string()).unwrap();
        assert!(c["policy"].get("merge_without_approval").is_none());
        assert_eq!(c["policy"]["other"].as_integer(), Some(1));
        assert!(c.get("signatures").is_none());
    }
}
