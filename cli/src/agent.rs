//! Starting an equipped agent in a library: `stemma claude` and `stemma codex`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::json;

use crate::library::Library;

/// The instructions every agent works with.
const INSTRUCTIONS: &str = include_str!("../agent/instructions.md");

/// The skills, as (name, `SKILL.md`).
const SKILLS: &[(&str, &str)] = &[
    (
        "stemma-mathematics",
        include_str!("../agent/skills/stemma-mathematics/SKILL.md"),
    ),
    (
        "stemma-documents",
        include_str!("../agent/skills/stemma-documents/SKILL.md"),
    ),
    (
        "stemma-sharing",
        include_str!("../agent/skills/stemma-sharing/SKILL.md"),
    ),
    (
        "stemma-signatures",
        include_str!("../agent/skills/stemma-signatures/SKILL.md"),
    ),
];

/// The files only `stemma` writes, which agents may not edit.
const PROTECTED: &[&str] = &[
    "stemma.toml",
    "lakefile.toml",
    "lake-manifest.json",
    "lean-toolchain",
    ".gitignore",
    ".github/**",
    "signatures/**",
];

/// Shell commands agents may not run.
const FORBIDDEN_COMMANDS: &[&str] = &[
    "stemma sign",
    "stemma key",
    "git push --force",
    "git push -f",
    "git push --force-with-lease",
    "git push origin main",
    "git push origin HEAD:main",
];

pub use crate::interact::SESSION_VARIABLE;

/// The agents `stemma` can start.
#[derive(Clone, Copy)]
pub enum Agent {
    Claude,
    Codex,
}

/// A prepared command, before it is run.
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub dir: PathBuf,
    pub branch: Option<String>,
}

/// What an agent should know when its session starts, cheap to compute: no
/// build, only git.
pub fn session_summary(library: &Library) -> String {
    let dir = &library.dir;
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let config = &library.config.library;
    let mut lines = vec![format!(
        "This session is equipped by stemma, in the library {} ({}).",
        config.title, config.name
    )];
    if let Some(branch) = git(&["branch", "--show-current"]) {
        lines.push(format!("- Branch: {branch}."));
    }
    if let Some(changes) = git(&["status", "--porcelain"]) {
        let n = changes.lines().count();
        if n > 0 {
            lines.push(format!("- {n} files have uncommitted changes."));
        }
    }
    if let Some(ahead) = git(&["rev-list", "--count", "origin/main..HEAD"])
        && ahead != "0"
    {
        lines.push(format!("- {ahead} commits are not in main yet."));
    }
    lines.push("Run `stemma status --json` when you need the state of the library.".into());
    lines.join("\n")
}

/// The command that runs this `stemma`, for hooks.
fn this_stemma() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "stemma".into())
}

/// The Claude Code settings for a session: what the agent may not do, and the
/// summary it gets when its session starts (or resumes, or is compacted).
fn claude_settings(library: &Path) -> serde_json::Value {
    let mut deny = Vec::new();
    for file in PROTECTED {
        // `//` makes the path absolute in Claude Code's permission rules.
        let path = format!("/{}/{file}", library.display());
        // Edit rules cover every file-editing tool.
        deny.push(format!("Edit({path})"));
    }
    for command in FORBIDDEN_COMMANDS {
        deny.push(format!("Bash({command}:*)"));
    }
    let hook = format!("'{}' hook session-start", this_stemma());
    json!({
        "permissions": { "deny": deny },
        "env": { SESSION_VARIABLE: "1" },
        "hooks": {
            "SessionStart": [{ "hooks": [{ "type": "command", "command": hook }] }],
        },
    })
}

/// Writes the session's files under `.stemma/agent/claude/` and returns the
/// arguments that load them.
fn prepare_claude(library: &Library) -> Result<Vec<String>> {
    let base = library.dir.join(".stemma").join("agent").join("claude");
    let plugin = base.join("plugin");
    std::fs::create_dir_all(plugin.join(".claude-plugin"))
        .with_context(|| format!("creating {}", plugin.display()))?;
    let settings = base.join("settings.json");
    std::fs::write(
        &settings,
        serde_json::to_string_pretty(&claude_settings(&library.dir))?,
    )?;
    let instructions = base.join("instructions.md");
    std::fs::write(&instructions, INSTRUCTIONS)?;
    std::fs::write(
        plugin.join(".claude-plugin").join("plugin.json"),
        serde_json::to_string_pretty(&json!({
            "name": "stemma",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "How to work in a Stemma library.",
        }))?,
    )?;
    for (name, skill) in SKILLS {
        let dir = plugin.join("skills").join(name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("SKILL.md"), skill)?;
    }
    Ok(vec![
        "--settings".into(),
        settings.display().to_string(),
        "--plugin-dir".into(),
        plugin.display().to_string(),
        "--append-system-prompt-file".into(),
        instructions.display().to_string(),
    ])
}

/// The instructions for Codex, which takes them as one text: the instructions,
/// then every skill.
fn codex_instructions() -> String {
    let mut text = INSTRUCTIONS.to_string();
    for (_, skill) in SKILLS {
        // Drop the skill's front matter: Codex reads the text itself.
        let body = skill
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---\n"))
            .map_or(*skill, |(_, body)| body);
        text.push_str("\n\n");
        text.push_str(body.trim());
    }
    text
}

/// The arguments that give Codex its instructions and sandbox. Codex runs
/// hooks only once a person trusts them, so the summary a Claude Code session
/// gets from its hook goes into Codex's instructions instead.
fn prepare_codex(library: &Library) -> Vec<String> {
    let text = format!("{}\n\n{}", codex_instructions(), session_summary(library));
    let instructions = toml::Value::String(text).to_string();
    vec![
        "--sandbox".into(),
        "workspace-write".into(),
        "--config".into(),
        format!("developer_instructions={instructions}"),
    ]
}

/// The person's handle: their account on the forge when it is known,
/// otherwise their git name, simplified.
pub fn person() -> String {
    if let Some(login) = crate::forge::current().login() {
        return login;
    }
    let name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let slug: String = name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() { "me".into() } else { slug }
}

/// Prepares the session of an agent in the library containing the current
/// directory.
pub fn prepare(
    agent: Agent,
    extra: Vec<String>,
    dry_run: bool,
    branch: &crate::branches::Request,
    mode: crate::interact::Mode,
) -> Result<Launch> {
    let library = Library::find(Path::new("."))?;
    if !dry_run && crate::agents_md::ensure(&library.dir, &library.config.library.title)? {
        crate::ui::note("Updated the block stemma keeps in AGENTS.md.");
    }
    let mut args = match agent {
        Agent::Claude => prepare_claude(&library)?,
        Agent::Codex => prepare_codex(&library),
    };
    args.extend(extra);
    let branch = if dry_run {
        crate::branches::current(&library.dir)
    } else {
        crate::branches::choose(&library.dir, branch, mode)?
    };
    let program = match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    };
    Ok(Launch {
        program: program.into(),
        args,
        dir: library.dir,
        branch,
    })
}

impl Launch {
    /// Runs the agent in place of `stemma`, so that it owns the terminal.
    pub fn run(self) -> Result<bool> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .current_dir(&self.dir)
            .env(SESSION_VARIABLE, "1");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = command.exec();
            Err(error).with_context(|| format!("starting `{}`; is it installed?", self.program))
        }
        #[cfg(not(unix))]
        {
            let status = command
                .status()
                .with_context(|| format!("starting `{}`; is it installed?", self.program))?;
            Ok(status.success())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_settings_deny_protected_files_and_signing() {
        let settings = claude_settings(Path::new("/lib"));
        let deny: Vec<&str> = settings["permissions"]["deny"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(deny.contains(&"Edit(//lib/lakefile.toml)"));
        assert!(deny.contains(&"Edit(//lib/signatures/**)"));
        assert!(deny.contains(&"Bash(stemma sign:*)"));
        assert_eq!(settings["env"][SESSION_VARIABLE], "1");
    }

    #[test]
    fn codex_instructions_include_every_skill_without_front_matter() {
        let text = codex_instructions();
        assert!(text.starts_with("# Working in a Stemma library"));
        for heading in [
            "# Writing Stemma documents",
            "# Doing mathematics",
            "# Saving and sharing",
            "# Signatures",
        ] {
            assert!(text.contains(heading), "missing {heading}");
        }
        assert!(!text.contains("description:"));
    }
}
