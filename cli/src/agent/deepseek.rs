//! DeepSeek Harness (`dsh`), through a patch layer of its configuration
//! (`dsh --patch <file>`), which this adapter writes for each start:
//!
//! - the instructions, as the user-level `AGENTS.md` its instruction loader
//!   reads (`dshHome` of `@deepseek-ai/dsh-agent-instructions`), so that the
//!   library's own `AGENTS.md` still follows it;
//! - the skills, as a directory of its filesystem skill provider
//!   (`customSkillDirs` of `@deepseek-ai/dsh-skill-filesystem`);
//! - hooks, through its bridge for Claude Code hooks
//!   (`@deepseek-ai/dsh-hooks-claude-code`): the session's summary when a
//!   session starts, and a check of every tool call that denies editing
//!   protected files and running forbidden commands.
//!
//! Its own sandbox (`workspace-write` by default) keeps writes inside the
//! workspace; `STEMMA_SESSION` reaches its shell commands, which keeps
//! `stemma sign` and `stemma key` from running.

use anyhow::Result;
use serde_json::json;

use super::{Equipment, Harness, INSTRUCTIONS, Setup, this_stemma};

/// The profile `dsh` boots when the person names none: its Web UI.
const DEFAULT_PROFILE: &str = "web";

/// The patch layer, as YAML (written as JSON, which YAML reads alike).
fn patch(home: &str, skills: &str, hooks: &str) -> serde_json::Value {
    json!([
        {
            "id": "agent-instructions",
            "name": "@deepseek-ai/dsh-agent-instructions",
            "config": { "maxBytes": 65536, "dshHome": home },
        },
        {
            "id": "skill-filesystem",
            "name": "@deepseek-ai/dsh-skill-filesystem",
            "config": { "customSkillDirs": [skills] },
        },
        {
            "insert": [{
                "id": "stemma-hooks",
                "name": "@deepseek-ai/dsh-hooks-claude-code",
                "config": { "configPath": hooks },
            }],
        },
    ])
}

/// The hooks, in Claude Code's format, which the bridge reads.
fn hooks() -> serde_json::Value {
    let stemma = this_stemma();
    json!({ "hooks": {
        "SessionStart": [{ "hooks": [{
            "type": "command", "command": format!("{stemma} --json hook session-start"),
        }] }],
        "PreToolUse": [{ "hooks": [{
            "type": "command", "command": format!("{stemma} hook pre-tool-use"),
        }] }],
    }})
}

/// Writes the session's files under `.stemma/agent/deepseek/`, and returns
/// the launcher's arguments that load them.
pub fn prepare(equipment: &Equipment) -> Result<Setup> {
    let base = equipment.dir(Harness::Deepseek)?;
    let home = base.join("home");
    std::fs::create_dir_all(&home)?;
    std::fs::write(home.join("AGENTS.md"), INSTRUCTIONS)?;
    let skills = base.join("skills");
    if skills.exists() {
        std::fs::remove_dir_all(&skills)?;
    }
    equipment.write_skills(&skills)?;
    let hooks_path = base.join("hooks.json");
    std::fs::write(&hooks_path, serde_json::to_string_pretty(&hooks())?)?;
    let patch_path = base.join("cordis.patch.yml");
    let text = format!(
        "# Written by `stemma agent deepseek` at each start; do not edit.\n{}\n",
        serde_json::to_string_pretty(&patch(
            &home.display().to_string(),
            &skills.display().to_string(),
            &hooks_path.display().to_string(),
        ))?
    );
    std::fs::write(&patch_path, text)?;
    Ok(Setup {
        args: vec!["--patch".into(), patch_path.display().to_string()],
        env: Vec::new(),
    })
}

/// The launcher's arguments: a profile first (the one the person names, as
/// `<name>` or `--profile <name>`, or the Web UI), the adapter's, then the
/// person's, which `dsh` hands to the profile's app.
pub fn arguments(setup: Vec<String>, extra: Vec<String>) -> Vec<String> {
    let names_profile = extra
        .iter()
        .any(|a| a == "--profile" || a.starts_with("--profile="));
    let mut args = Vec::new();
    let mut extra = extra.into_iter().peekable();
    if !names_profile {
        match extra.peek() {
            Some(first) if !first.starts_with('-') => args.push(extra.next().unwrap()),
            _ => args.push(DEFAULT_PROFILE.into()),
        }
    }
    args.extend(setup);
    args.extend(extra);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_profile_comes_first() {
        let setup = strings(&["--patch", "p.yml"]);
        assert_eq!(
            arguments(setup.clone(), vec![]),
            ["web", "--patch", "p.yml"]
        );
        assert_eq!(
            arguments(setup.clone(), strings(&["--no-open"])),
            ["web", "--patch", "p.yml", "--no-open"]
        );
        assert_eq!(
            arguments(setup.clone(), strings(&["tui", "--resume", "x"])),
            ["tui", "--patch", "p.yml", "--resume", "x"]
        );
        assert_eq!(
            arguments(setup, strings(&["--profile", "tui"])),
            ["--patch", "p.yml", "--profile", "tui"]
        );
    }

    #[test]
    fn the_patch_equips_instructions_skills_and_hooks() {
        let p = patch("/h", "/s", "/k.json");
        assert_eq!(p[0]["config"]["dshHome"], "/h");
        assert_eq!(p[0]["config"]["maxBytes"], 65536);
        assert_eq!(p[1]["config"]["customSkillDirs"][0], "/s");
        assert_eq!(
            p[2]["insert"][0]["name"],
            "@deepseek-ai/dsh-hooks-claude-code"
        );
        let h = hooks();
        assert!(
            h["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .ends_with("--json hook session-start")
        );
    }
}
