//! Starting an equipped agent in a library: `stemma agent <harness>`, and its
//! shortcuts `stemma claude` and `stemma codex`.
//!
//! The **equipment** is the same for every harness, and written once, in a
//! neutral form: the system instructions, the skills, the summary an agent
//! gets when its session starts, the files it may not edit and the commands it
//! may not run. Each harness gets a thin **adapter** (the submodules) that
//! installs that equipment the way the harness reads it: instructions, skills,
//! hooks or their equivalent, environment variables and permission rules.
//! Every harness runs with [`SESSION_VARIABLE`] set, under which `stemma sign`
//! and `stemma key` refuse to run.
//!
//! Choosing the branch to work on is not an adapter's business: it is
//! [`crate::branches::choose`]'s.

mod claude;
mod codex;
mod deepseek;
mod opencode;

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::json;

use crate::library::Library;

pub use crate::interact::SESSION_VARIABLE;

/// The instructions every agent works with. Their first heading is the
/// section the guard in `AGENTS.md` names.
pub const INSTRUCTIONS: &str = include_str!("../agent/instructions.md");

/// The title of the section an equipped agent's instructions contain.
pub const INSTRUCTIONS_TITLE: &str = "Working in a Stemma library";

/// The skills, as (name, `SKILL.md`).
pub const SKILLS: &[(&str, &str)] = &[
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

/// The files only `stemma` writes, which agents may not edit, relative to the
/// library: a file, or a directory and everything in it (`dir/**`).
pub const PROTECTED: &[&str] = &[
    "stemma.toml",
    "lakefile.toml",
    "lake-manifest.json",
    "lean-toolchain",
    ".gitignore",
    ".github/**",
    "signatures/**",
];

/// Shell commands agents may not run, as prefixes.
pub const FORBIDDEN_COMMANDS: &[&str] = &[
    "stemma sign",
    "stemma key",
    "git push --force",
    "git push -f",
    "git push --force-with-lease",
    "git push origin main",
    "git push origin HEAD:main",
];

/// The harnesses `stemma` can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Harness {
    /// Claude Code.
    Claude,
    /// Codex.
    Codex,
    /// DeepSeek Harness (`dsh`).
    Deepseek,
    /// OpenCode.
    Opencode,
}

impl Harness {
    /// The name `stemma agent` takes.
    pub fn name(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Deepseek => "deepseek",
            Harness::Opencode => "opencode",
        }
    }

    /// The program that starts it.
    pub fn program(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Deepseek => "dsh",
            Harness::Opencode => "opencode",
        }
    }
}

/// What an adapter gives the harness: arguments before the person's own, and
/// environment variables besides [`SESSION_VARIABLE`].
#[derive(Default)]
pub struct Setup {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The equipment of a session in a library, which adapters install.
pub struct Equipment<'a> {
    pub library: &'a Library,
}

impl Equipment<'_> {
    /// Where the files of a harness's session go: `.stemma/agent/<harness>/`,
    /// written anew at each start.
    pub fn dir(&self, harness: Harness) -> Result<PathBuf> {
        let dir = self
            .library
            .dir
            .join(".stemma")
            .join("agent")
            .join(harness.name());
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(dir)
    }

    /// Writes the neutral equipment, the instructions and the skills, to
    /// `.stemma/agent/equipment/`, where a person can also take it to equip a
    /// harness `stemma` does not support. Returns that directory.
    pub fn write_neutral(&self) -> Result<PathBuf> {
        let dir = self
            .library
            .dir
            .join(".stemma")
            .join("agent")
            .join("equipment");
        let skills = dir.join("skills");
        if skills.exists() {
            std::fs::remove_dir_all(&skills)?;
        }
        self.write_skills(&skills)?;
        std::fs::write(dir.join("instructions.md"), INSTRUCTIONS)?;
        Ok(dir)
    }

    /// Writes every skill to `<dir>/<name>/SKILL.md`, the layout every
    /// harness that reads skills shares.
    pub fn write_skills(&self, dir: &Path) -> Result<()> {
        for (name, skill) in SKILLS {
            let dir = dir.join(name);
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            std::fs::write(dir.join("SKILL.md"), skill)?;
        }
        Ok(())
    }

    /// The summary an agent gets when its session starts.
    pub fn summary(&self) -> String {
        session_summary(self.library)
    }

    /// The instructions and every skill as one text, for a harness that reads
    /// no skills, followed by the session's summary.
    pub fn inline_instructions(&self) -> String {
        format!("{}\n\n{}", inline_instructions(), self.summary())
    }
}

/// The instructions, then every skill without its front matter.
fn inline_instructions() -> String {
    let mut text = INSTRUCTIONS.to_string();
    for (_, skill) in SKILLS {
        let body = skill
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---\n"))
            .map_or(*skill, |(_, body)| body);
        text.push_str("\n\n");
        text.push_str(body.trim());
    }
    text
}

/// A prepared command, before it is run.
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// Environment variables, [`SESSION_VARIABLE`] among them.
    pub env: Vec<(String, String)>,
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

/// The command that runs this `stemma`, for hooks, quoted for a shell.
pub fn this_stemma() -> String {
    let path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "stemma".into());
    format!("'{}'", path.replace('\'', r"'\''"))
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
    harness: Harness,
    extra: Vec<String>,
    dry_run: bool,
    branch: &crate::branches::Request,
    mode: crate::interact::Mode,
) -> Result<Launch> {
    let library = Library::find(Path::new("."))?;
    if !dry_run && crate::agents_md::ensure(&library.dir, &library.config.library.title)? {
        crate::ui::note("Updated the block stemma keeps in AGENTS.md.");
    }
    // The branch first: the summary an adapter writes is the chosen branch's.
    let branch = if dry_run {
        crate::branches::current(&library.dir)
    } else {
        crate::branches::choose(&library.dir, branch, mode)?
    };
    let equipment = Equipment { library: &library };
    equipment.write_neutral()?;
    let setup = match harness {
        Harness::Claude => claude::prepare(&equipment)?,
        Harness::Codex => codex::prepare(&equipment)?,
        Harness::Deepseek => deepseek::prepare(&equipment)?,
        Harness::Opencode => opencode::prepare(&equipment)?,
    };
    let args = match harness {
        Harness::Deepseek => deepseek::arguments(setup.args, extra),
        _ => setup.args.into_iter().chain(extra).collect(),
    };
    let mut env = vec![(SESSION_VARIABLE.to_string(), "1".to_string())];
    env.extend(setup.env);
    Ok(Launch {
        program: harness.program().into(),
        args,
        env,
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
            .envs(self.env.iter().map(|(k, v)| (k, v)));
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

// Hooks: what harnesses run at moments of a session.

/// Runs a hook for an agent. `session-start` prints the session's summary (as
/// Claude Code's hook JSON with `json_output`); `pre-tool-use` reads a tool
/// call, in Claude Code's hook format, from stdin, and denies it when it would
/// edit a protected file or run a forbidden command.
pub fn hook(event: &str, json_output: bool) -> Result<bool> {
    match event {
        "session-start" => {
            let library = Library::find(Path::new("."))?;
            let summary = session_summary(&library);
            if json_output {
                let value = json!({ "hookSpecificOutput": {
                    "hookEventName": "SessionStart", "additionalContext": summary,
                }});
                println!("{value}");
            } else {
                println!("{summary}");
            }
            Ok(true)
        }
        "pre-tool-use" => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let call: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
            if let Some(reason) = denial(&call) {
                let value = json!({ "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }});
                println!("{value}");
            }
            Ok(true)
        }
        other => anyhow::bail!("unknown hook event '{other}'"),
    }
}

/// Why a tool call, in Claude Code's hook format (`tool_name`, `tool_input`,
/// `cwd`), is denied, if it is.
pub fn denial(call: &serde_json::Value) -> Option<String> {
    let tool = call["tool_name"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    let input = &call["tool_input"];
    let cwd = call["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    if let Some(command) = input["command"].as_str()
        && matches!(tool.as_str(), "bash" | "shell" | "pwsh" | "exec_command")
    {
        return forbidden_command(command).map(|f| {
            format!(
                "`{f}` is a person's act: agents started by stemma never run it. Tell the person \
                 what to run in their own terminal."
            )
        });
    }
    let editing = match tool.as_str() {
        "write" | "edit" | "multiedit" | "notebookedit" | "apply_patch" => true,
        // `view` only reads.
        "str_replace_editor" => input["command"].as_str() != Some("view"),
        _ => false,
    };
    if !editing {
        return None;
    }
    let library = Library::find(&cwd).ok()?;
    ["file_path", "path", "notebook_path"]
        .iter()
        .filter_map(|key| input[key].as_str())
        .find_map(|path| protected_file(&library.dir, &cwd.join(path)))
        .map(|file| {
            format!(
                "{file} is written only by stemma (or, for signatures, by a person signing): \
                 agents never edit it."
            )
        })
}

/// Removes `.` and `..` from a path, without reading the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A path with its longest existing ancestor resolved, so that a library and
/// a file in it compare alike through symbolic links.
fn resolve(path: &Path) -> PathBuf {
    let path = normalize(path);
    let mut rest = Vec::new();
    let mut base = path.as_path();
    loop {
        if let Ok(real) = base.canonicalize() {
            return rest.iter().rev().fold(real, |p, c| p.join(c));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                base = parent;
            }
            _ => return path,
        }
    }
}

/// The protected file `path` is, relative to the library at `library`.
pub fn protected_file(library: &Path, path: &Path) -> Option<String> {
    let relative = resolve(path)
        .strip_prefix(resolve(library))
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    PROTECTED
        .iter()
        .any(|p| match p.strip_suffix("/**") {
            Some(dir) => relative == dir || relative.starts_with(&format!("{dir}/")),
            None => relative == *p,
        })
        .then_some(relative)
}

/// The forbidden command a shell command line runs, if any, by its simple
/// commands: `cd x && stemma sign` runs `stemma sign`. It is a guard against
/// shortcuts, not against an agent set on evading it: `stemma sign` and
/// `stemma key` refuse to run in an agent's session anyway.
pub fn forbidden_command(line: &str) -> Option<&'static str> {
    let separators = [';', '&', '|', '\n', '(', ')', '`', '{', '}'];
    line.split(|c| separators.contains(&c)).find_map(|simple| {
        let mut words = simple
            .split_whitespace()
            .map(|w| w.trim_matches(|c| c == '"' || c == '\''))
            .skip_while(|w| w.contains('=') || matches!(*w, "env" | "exec" | "command" | "$"));
        let program = words.next()?;
        let program = program.rsplit('/').next().unwrap_or(program);
        let rest: Vec<&str> = words.collect();
        match program {
            "stemma" => match rest.first() {
                Some(&"sign") => Some("stemma sign"),
                Some(&"key") => Some("stemma key"),
                _ => None,
            },
            "git" => {
                let push = rest.iter().position(|w| *w == "push")?;
                let args = &rest[push + 1..];
                let forced = args.iter().any(|a| {
                    *a == "-f" || a.starts_with("--force") || (a.starts_with('+') && a.len() > 1)
                });
                if forced {
                    return Some("git push --force");
                }
                args.iter()
                    .filter(|a| !a.starts_with('-'))
                    .skip(1)
                    .any(|a| {
                        let target = a.rsplit(':').next().unwrap_or(a);
                        target == "main" || target == "refs/heads/main"
                    })
                    .then_some("git push origin main")
            }
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instructions_name_the_section_the_guard_looks_for() {
        assert!(INSTRUCTIONS.starts_with(&format!("# {INSTRUCTIONS_TITLE}\n")));
        assert!(crate::agents_md::block().contains(INSTRUCTIONS_TITLE));
    }

    #[test]
    fn inline_instructions_include_every_skill_without_front_matter() {
        let text = inline_instructions();
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

    #[test]
    fn forbidden_commands_are_found_inside_command_lines() {
        for line in [
            "stemma sign",
            "stemma sign even-add",
            "cd lib && stemma sign",
            "/usr/local/bin/stemma key add",
            "STEMMA_SESSION= stemma sign",
            "git push --force",
            "git push -f origin work/alice",
            "git push --force-with-lease",
            "git push origin main",
            "git push origin HEAD:main",
            "git -C lib push origin main",
            "git push origin +work/alice",
            "echo hi; git push upstream refs/heads/main",
        ] {
            assert!(forbidden_command(line).is_some(), "{line}");
        }
        for line in [
            "stemma check --json",
            "stemma share --json",
            "git push",
            "git push origin work/alice",
            "git push --set-upstream origin work/alice-main",
            "echo stemma sign",
            "git log main",
        ] {
            assert_eq!(forbidden_command(line), None, "{line}");
        }
    }

    #[test]
    fn protected_files_are_matched_relative_to_the_library() {
        let lib = Path::new("/nonexistent/lib");
        for (path, protected) in [
            ("/nonexistent/lib/stemma.toml", true),
            ("/nonexistent/lib/./signatures/even.toml", true),
            ("/nonexistent/lib/.github/workflows/stemma.yml", true),
            ("/nonexistent/lib/Alg/../lakefile.toml", true),
            ("/nonexistent/lib/Alg/Even.lean", false),
            ("/nonexistent/lib/signaturesx", false),
            ("/nonexistent/other/stemma.toml", false),
        ] {
            assert_eq!(
                protected_file(lib, Path::new(path)).is_some(),
                protected,
                "{path}"
            );
        }
    }
}
