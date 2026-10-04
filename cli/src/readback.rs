//! `stemma readback`: blind translations of environments' Lean into prose, a
//! voluntary aid to auditing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::library::Library;
use crate::report::{Environment, Report};
use crate::site;

/// A read-back, as kept in `.stemma/readbacks/<label>.json`.
#[derive(Serialize, Deserialize)]
struct Readback {
    label: String,
    display: String,
    module: String,
    /// The formal fingerprint the read-back was made from.
    formal: String,
    prose: String,
    lean: String,
    readback: String,
    agent: String,
}

/// Which agent makes the read-backs.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Translator {
    Claude,
    Codex,
}

/// Options of `stemma readback`.
pub struct Options {
    pub labels: Vec<String>,
    pub module: Option<String>,
    pub force: bool,
    pub translator: Translator,
    pub model: Option<String>,
    pub serve: bool,
    pub port: u16,
}

fn readbacks_dir(library: &Library) -> PathBuf {
    library.dir.join(".stemma").join("readbacks")
}

/// The formal side of an environment: its Lean, and the Lean of the library's
/// environments that define what it uses. Never its prose.
fn formal_side(report: &Report, env: &Environment) -> String {
    let mut parts = vec![env.record.lean.clone()];
    for (name, _) in &env.closure {
        if env.record.decls.contains(name) {
            continue;
        }
        if let Some(other) = report
            .environments
            .iter()
            .find(|e| e.record.decls.contains(name))
            && !parts.contains(&other.record.lean)
        {
            parts.push(other.record.lean.clone());
        }
    }
    parts.reverse();
    parts.join("\n\n")
}

/// The request a fresh agent session answers.
fn prompt(formal: &str) -> String {
    format!(
        "You are auditing a formalization. Below is Lean 4 code (with Mathlib \
conventions). Read only the code and translate the LAST declaration's statement \
(for a definition: what it defines) into precise mathematical prose, as a \
mathematician would state it in a paper. Do not guess intent from names: say \
exactly what the Lean says, including hypotheses, quantifiers and types. Then, \
under a heading \"Notes\", point out anything that makes the statement differ \
from what a reader might expect: Mathlib conventions (truncated subtraction on \
natural numbers, division by zero, junk values), missing or unusual hypotheses, \
or vacuous cases. Write \"Notes: none\" if there is nothing. Answer with the \
prose and the notes only.\n\n{formal}\n"
    )
}

/// Asks a fresh agent session, in an empty directory, for a read-back.
fn translate(translator: Translator, model: Option<&str>, text: &str) -> Result<String> {
    let empty = std::env::temp_dir().join(format!("stemma-readback-{}", std::process::id()));
    std::fs::create_dir_all(&empty)?;
    // Codex writes its final answer to a file of its own.
    let answer = empty.join("answer.md");
    let _ = std::fs::remove_file(&answer);
    let mut command = match translator {
        Translator::Claude => {
            let mut c = Command::new("claude");
            c.args(["-p", text]);
            if let Some(m) = model {
                c.args(["--model", m]);
            }
            c
        }
        Translator::Codex => {
            let mut c = Command::new("codex");
            c.args(["exec", "--sandbox", "read-only", "--skip-git-repo-check"]);
            c.arg("--output-last-message").arg(&answer);
            if let Some(m) = model {
                c.args(["--model", m]);
            }
            c.arg(text);
            c
        }
    };
    // A read-back is made outside any agent's session, even one that asked for it.
    let output = command
        .current_dir(&empty)
        .env_remove("CLAUDECODE")
        .env_remove(crate::agent::SESSION_VARIABLE)
        .stdin(Stdio::null())
        .output()
        .context("starting the agent that makes read-backs; is it installed?")?;
    if !output.status.success() {
        bail!(
            "the agent failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(match translator {
        Translator::Claude => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Translator::Codex => std::fs::read_to_string(&answer)
            .context("reading Codex's answer")?
            .trim()
            .to_string(),
    })
}

/// Escapes text for HTML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The page that sets every read-back beside its environment's prose.
fn page(title: &str, readbacks: &[Readback]) -> String {
    let mut body = String::new();
    for r in readbacks {
        body.push_str(&format!(
            "<section><h2>{} <code>{}</code> <small>{}</small></h2>\
<div class=\"pair\"><div><h3>The prose</h3><p>{}</p></div>\
<div><h3>The read-back of the Lean</h3><p>{}</p></div></div>\
<details><summary>Lean</summary><pre>{}</pre></details></section>",
            escape(&r.display),
            escape(&r.label),
            escape(&r.module),
            escape(&r.prose),
            escape(&r.readback),
            escape(&r.lean),
        ));
    }
    if readbacks.is_empty() {
        body.push_str("<p>No read-backs yet.</p>");
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Read-backs: {t}</title>\
<style>body{{font-family:system-ui,sans-serif;max-width:72rem;margin:2rem auto;padding:0 1rem;color:#1f2328}}\
.pair{{display:grid;grid-template-columns:1fr 1fr;gap:1.5rem}}\
.pair p{{white-space:pre-wrap;line-height:1.5}}\
h3{{font-size:.8rem;text-transform:none;color:#59636e;margin:0 0 .3rem}}\
section{{border-top:1px solid #d1d9e0;padding:1rem 0}}small{{color:#59636e;font-weight:normal}}\
pre{{background:#f6f8fa;padding:.8rem;overflow-x:auto}}\
@media(max-width:700px){{.pair{{grid-template-columns:1fr}}}}\
@media(prefers-color-scheme:dark){{body{{background:#0d1117;color:#e6edf3}}pre{{background:#161b22}}}}</style>\
</head><body><h1>Read-backs: {t}</h1><p>Each read-back translates an environment's \
Lean into prose without seeing its prose. Compare the two.</p>{body}</body></html>",
        t = escape(title)
    )
}

/// `stemma readback`.
pub fn readback(options: Options, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let report = match crate::commands::build_and_extract(&library, json_output)? {
        Ok(report) => report,
        Err(output) => {
            crate::commands::show_build_errors(&output);
            return Ok(false);
        }
    };
    let dir = readbacks_dir(&library);
    std::fs::create_dir_all(&dir)?;
    let chosen: Vec<&Environment> = report
        .environments
        .iter()
        .filter(|e| e.fingerprints.is_some())
        .filter(|e| {
            let label = e.record.label.clone().unwrap_or_default();
            if !options.labels.is_empty() {
                options.labels.contains(&label)
            } else if let Some(m) = &options.module {
                e.record.module == *m || e.record.module.ends_with(&format!(".{m}"))
            } else {
                e.is_central_claim()
            }
        })
        .collect();
    let mut made = Vec::new();
    for env in chosen {
        let label = env.record.label.clone().unwrap_or_default();
        let formal = env
            .fingerprints
            .as_ref()
            .map(|f| f.formal.clone())
            .unwrap_or_default();
        let path = dir.join(format!("{label}.json"));
        let current = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Readback>(&t).ok())
            .is_some_and(|r| r.formal == formal);
        if current && !options.force {
            continue;
        }
        let spinner = crate::ui::Spinner::start(format!("Reading back {label}"), json_output);
        let text = translate(
            options.translator,
            options.model.as_deref(),
            &prompt(&formal_side(&report, env)),
        );
        spinner.stop();
        let text = text?;
        let r = Readback {
            label: label.clone(),
            display: env.record.display.clone(),
            module: env.record.module.clone(),
            formal,
            prose: env.record.prose.clone(),
            lean: env.record.lean.clone(),
            readback: text,
            agent: match options.translator {
                Translator::Claude => "claude".into(),
                Translator::Codex => "codex".into(),
            },
        };
        std::fs::write(&path, serde_json::to_string_pretty(&r)?)?;
        made.push(label);
    }
    // The page shows every read-back still current.
    let mut all = Vec::new();
    for env in &report.environments {
        let (Some(label), Some(fp)) = (&env.record.label, &env.fingerprints) else {
            continue;
        };
        if let Some(r) = std::fs::read_to_string(dir.join(format!("{label}.json")))
            .ok()
            .and_then(|t| serde_json::from_str::<Readback>(&t).ok())
            .filter(|r| r.formal == fp.formal)
        {
            all.push(r);
        }
    }
    let index = dir.join("index.html");
    std::fs::write(&index, page(&library.config.library.title, &all))?;
    if !options.serve {
        if json_output {
            println!("{}", json!({ "made": made, "page": index }));
        } else {
            crate::ui::success(format!(
                "Made {} read-backs {}",
                made.len(),
                crate::ui::dim(index.display())
            ));
        }
        return Ok(true);
    }
    let (listener, port) = site::bind(options.port)?;
    let url = format!("http://127.0.0.1:{port}/");
    if json_output {
        println!("{}", json!({ "made": made, "page": index, "url": url }));
    } else {
        crate::ui::success(format!(
            "Made {} read-backs. Serving them at {}",
            made.len(),
            crate::ui::bold(&url)
        ));
        crate::ui::note("Ctrl-C to stop.");
    }
    for stream in listener.incoming().flatten() {
        let _ = site::respond(stream, &dir);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_escapes_html() {
        let r = Readback {
            label: "x".into(),
            display: "Theorem".into(),
            module: "M".into(),
            formal: String::new(),
            prose: "a < b".into(),
            lean: String::new(),
            readback: "<script>".into(),
            agent: "claude".into(),
        };
        let html = page("T", &[r]);
        assert!(html.contains("a &lt; b") && html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
