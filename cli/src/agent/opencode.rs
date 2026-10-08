//! OpenCode, through configuration it merges over the person's own, given in
//! `OPENCODE_CONFIG_CONTENT` (which outranks the global and the project
//! configuration):
//!
//! - `instructions`: a file with the instructions and the session's summary
//!   (OpenCode has no hook that adds to the context when a session starts, so
//!   the summary is the one of the start, as for Codex);
//! - `skills.paths`: the directory of the skills;
//! - `permission`: `edit` denied on protected files (it covers every tool that
//!   writes files: edit, write, apply_patch), and `bash` denied for forbidden
//!   commands.
//!
//! `STEMMA_SESSION` reaches its shell commands, which keeps `stemma sign` and
//! `stemma key` from running.

use anyhow::Result;
use serde_json::json;

use super::{Equipment, FORBIDDEN_COMMANDS, Harness, PROTECTED, Setup};

/// The variable OpenCode reads inline configuration from.
pub const CONFIG_VARIABLE: &str = "OPENCODE_CONFIG_CONTENT";

/// The configuration of a session.
fn config(instructions: &str, skills: &str) -> serde_json::Value {
    // Patterns match paths relative to the project, and commands; `*`
    // matches anything, and a trailing ` *` also matches nothing.
    let mut edit = serde_json::Map::new();
    for file in PROTECTED {
        let pattern = match file.strip_suffix("/**") {
            Some(dir) => format!("{dir}/*"),
            None => file.to_string(),
        };
        edit.insert(pattern, json!("deny"));
    }
    let mut bash = serde_json::Map::new();
    for command in FORBIDDEN_COMMANDS {
        bash.insert(format!("{command} *"), json!("deny"));
        // The same command run by its path.
        bash.insert(format!("*/{command} *"), json!("deny"));
    }
    json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": [instructions],
        "skills": { "paths": [skills] },
        "permission": { "edit": edit, "bash": bash },
    })
}

/// Writes the session's files under `.stemma/agent/opencode/`, and returns
/// the configuration that loads them.
pub fn prepare(equipment: &Equipment) -> Result<Setup> {
    let base = equipment.dir(Harness::Opencode)?;
    let instructions = base.join("instructions.md");
    std::fs::write(
        &instructions,
        format!(
            "{}\n\n{}\n",
            super::INSTRUCTIONS.trim_end(),
            equipment.summary()
        ),
    )?;
    let skills = base.join("skills");
    if skills.exists() {
        std::fs::remove_dir_all(&skills)?;
    }
    equipment.write_skills(&skills)?;
    let config = config(
        &instructions.display().to_string(),
        &skills.display().to_string(),
    );
    let text = serde_json::to_string_pretty(&config)?;
    // For a person to read; OpenCode reads the variable.
    std::fs::write(base.join("opencode.json"), &text)?;
    Ok(Setup {
        args: Vec::new(),
        env: vec![(CONFIG_VARIABLE.into(), text)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configuration_denies_protected_files_and_signing() {
        let c = config("/i.md", "/skills");
        assert_eq!(c["instructions"][0], "/i.md");
        assert_eq!(c["skills"]["paths"][0], "/skills");
        assert_eq!(c["permission"]["edit"]["stemma.toml"], "deny");
        assert_eq!(c["permission"]["edit"]["signatures/*"], "deny");
        assert_eq!(c["permission"]["edit"][".github/*"], "deny");
        assert_eq!(c["permission"]["bash"]["stemma sign *"], "deny");
        assert_eq!(c["permission"]["bash"]["git push --force *"], "deny");
        // Nothing else is decided for the person.
        assert!(c["permission"]["edit"].get("*").is_none());
    }
}
