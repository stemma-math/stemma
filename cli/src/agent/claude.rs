//! Claude Code: a plugin with the skills, settings that deny protected files
//! and forbidden commands, the instructions appended to its system prompt, and
//! hooks for the session's summary and a second check of every tool call.

use anyhow::{Context, Result};
use serde_json::json;

use super::{Equipment, FORBIDDEN_COMMANDS, Harness, INSTRUCTIONS, PROTECTED, Setup};
use super::{SESSION_VARIABLE, this_stemma};

/// The Claude Code settings for a session: what the agent may not do, and the
/// hooks it runs.
pub fn settings(library: &std::path::Path) -> serde_json::Value {
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
    let stemma = this_stemma();
    json!({
        "permissions": { "deny": deny },
        "env": { SESSION_VARIABLE: "1" },
        "hooks": {
            "SessionStart": [{ "hooks": [{
                "type": "command", "command": format!("{stemma} hook session-start"),
            }] }],
            "PreToolUse": [{ "hooks": [{
                "type": "command", "command": format!("{stemma} hook pre-tool-use"),
            }] }],
        },
    })
}

/// Writes the session's files under `.stemma/agent/claude/` and returns the
/// arguments that load them.
pub fn prepare(equipment: &Equipment) -> Result<Setup> {
    let base = equipment.dir(Harness::Claude)?;
    let plugin = base.join("plugin");
    if plugin.exists() {
        std::fs::remove_dir_all(&plugin)?;
    }
    std::fs::create_dir_all(plugin.join(".claude-plugin"))
        .with_context(|| format!("creating {}", plugin.display()))?;
    let settings_path = base.join("settings.json");
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings(&equipment.library.dir))?,
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
    equipment.write_skills(&plugin.join("skills"))?;
    Ok(Setup {
        args: vec![
            "--settings".into(),
            settings_path.display().to_string(),
            "--plugin-dir".into(),
            plugin.display().to_string(),
            "--append-system-prompt-file".into(),
            instructions.display().to_string(),
        ],
        env: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn settings_deny_protected_files_and_signing() {
        let settings = settings(Path::new("/lib"));
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
        let pre = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(pre.ends_with(" hook pre-tool-use"));
    }
}
