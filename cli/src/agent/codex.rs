//! Codex: the instructions, every skill and the session's summary in its
//! developer instructions, and its `workspace-write` sandbox. Codex runs
//! hooks only once a person trusts them, and has no rules for single files,
//! so it cannot deny protected files or commands: its instructions forbid
//! them, and `STEMMA_SESSION` keeps `stemma sign` and `stemma key` from
//! running.

use anyhow::Result;

use super::{Equipment, Harness, Setup};

/// The arguments that give Codex its instructions and sandbox. The
/// instructions are also written to `.stemma/agent/codex/instructions.md`, for
/// a person to read.
pub fn prepare(equipment: &Equipment) -> Result<Setup> {
    let text = equipment.inline_instructions();
    std::fs::write(
        equipment.dir(Harness::Codex)?.join("instructions.md"),
        &text,
    )?;
    let instructions = toml::Value::String(text).to_string();
    Ok(Setup {
        args: vec![
            "--sandbox".into(),
            "workspace-write".into(),
            "--config".into(),
            format!("developer_instructions={instructions}"),
        ],
        env: Vec::new(),
    })
}
