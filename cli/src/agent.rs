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
        "stemma-documents",
        include_str!("../agent/skills/stemma-documents/SKILL.md"),
    ),
    (
        "stemma-formalization",
        include_str!("../agent/skills/stemma-formalization/SKILL.md"),
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
    "AGENTS.md",
    ".gitignore",
    ".github/**",
    "signatures/**",
];

/// Shell commands agents may not run.
const FORBIDDEN_COMMANDS: &[&str] = &[
    "stemma sign",
    "git push --force",
    "git push -f",
    "git push --force-with-lease",
    "git push origin main",
    "git push origin HEAD:main",
];

/// The environment variable that marks a process as part of an agent's session.
pub const SESSION_VARIABLE: &str = "STEMMA_SESSION";

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

/// The Claude Code settings for a session: what the agent may not do.
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
    json!({
        "permissions": { "deny": deny },
        "env": { SESSION_VARIABLE: "1" },
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

/// The arguments that give Codex its instructions and sandbox.
fn prepare_codex() -> Vec<String> {
    let instructions = toml::Value::String(codex_instructions()).to_string();
    vec![
        "--sandbox".into(),
        "workspace-write".into(),
        "--config".into(),
        format!("developer_instructions={instructions}"),
    ]
}

/// The person's handle: their GitHub login when `gh` knows it, otherwise
/// their git name, simplified.
fn person() -> String {
    let gh = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output();
    if let Ok(out) = gh
        && out.status.success()
    {
        let login = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !login.is_empty() {
            return login;
        }
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

/// The current branch, if the library is a git repository.
fn current_branch(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Moves from `main` to the person's working branch, creating it the first time.
fn ensure_working_branch(dir: &Path) -> Result<Option<String>> {
    let Some(branch) = current_branch(dir) else {
        return Ok(None);
    };
    if branch != "main" {
        return Ok(Some(branch));
    }
    let working = format!("work/{}", person());
    let exists = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &working])
        .current_dir(dir)
        .output()?
        .status
        .success();
    let mut switch = Command::new("git");
    switch.arg("switch").current_dir(dir);
    if !exists {
        switch.arg("--create");
    }
    let status = switch.arg(&working).arg("--quiet").status()?;
    anyhow::ensure!(status.success(), "could not switch to the branch {working}");
    Ok(Some(working))
}

/// Prepares the session of an agent in the library containing the current
/// directory.
pub fn prepare(agent: Agent, extra: Vec<String>, dry_run: bool) -> Result<Launch> {
    let library = Library::find(Path::new("."))?;
    let mut args = match agent {
        Agent::Claude => prepare_claude(&library)?,
        Agent::Codex => prepare_codex(),
    };
    args.extend(extra);
    let branch = if dry_run {
        current_branch(&library.dir)
    } else {
        ensure_working_branch(&library.dir)?
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
            "# Formalizing",
            "# Saving and sharing",
            "# Signatures",
        ] {
            assert!(text.contains(heading), "missing {heading}");
        }
        assert!(!text.contains("description:"));
    }
}
