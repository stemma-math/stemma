//! `stemma readback`: blind translations of environments' Lean into prose, a
//! voluntary aid to auditing, made for a person to read first.

mod page;
mod reviews;
mod server;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::library::Library;
use crate::report::{Environment, Report};
use crate::site;

use reviews::Reviews;

/// A read-back, as kept in `.stemma/readbacks/<label>.json`.
#[derive(Clone, Serialize, Deserialize)]
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
    /// Print the read-backs' text instead of making any.
    pub content: bool,
    /// Print the person's marks and notes instead of making any.
    pub notes: bool,
}

/// Where read-backs are kept.
fn dir_of(library: &Path) -> PathBuf {
    library.join(".stemma").join("readbacks")
}

fn readbacks_dir(library: &Library) -> PathBuf {
    dir_of(&library.dir)
}

/// The read-back of a label kept on disk, if any.
fn load(dir: &Path, label: &str) -> Option<Readback> {
    std::fs::read_to_string(dir.join(format!("{label}.json")))
        .ok()
        .and_then(|t| serde_json::from_str::<Readback>(&t).ok())
        .filter(|r| r.label == label)
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
fn prompt(formal: &str, declarations: &[String]) -> String {
    let targets = declarations.join("\n");
    format!(
        "You are auditing a formalization. Below is Lean 4 code (with Mathlib \
conventions). Read only the code and translate EVERY declaration listed in \
Target declarations below (for a definition: what it defines) into precise \
mathematical prose, as a \
mathematician would state it in a paper. Do not guess intent from names: say \
exactly what the Lean says, including hypotheses, quantifiers and types. Then, \
under a heading \"Notes\", point out anything that makes the statement differ \
from what a reader might expect: Mathlib conventions (truncated subtraction on \
natural numbers, division by zero, junk values), missing or unusual hypotheses, \
or vacuous cases. Write \"Notes: none\" if there is nothing. Answer with the \
prose and the notes only. Cover each target, including structures, definitions, \
and instances; do not restrict the read-back to the last declaration. Other \
declarations are dependency context, not additional targets.\n\nTarget declarations:\n\
{targets}\n\nLean code:\n{formal}\n"
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

/// How a read-back stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum State {
    /// Made by this run of `stemma readback`.
    New,
    /// Made of the environment's Lean as it is now.
    Current,
    /// Made of Lean that has changed since, or of an environment that is gone.
    Stale,
    /// Put aside by the person, or because its environment was signed.
    Archived,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::New => "new",
            State::Current => "current",
            State::Stale => "stale",
            State::Archived => "archived",
        }
    }
}

/// A read-back on disk, and how it stands against the library.
struct Entry {
    readback: Readback,
    /// New, current or stale: archiving is the person's, and read separately.
    freshness: State,
    /// Whether its environment is gone from the library, or has no Lean.
    gone: bool,
}

impl Entry {
    fn label(&self) -> &str {
        &self.readback.label
    }

    fn state(&self, reviews: &Reviews) -> State {
        if reviews.get(self.label()).is_archived(&self.readback.formal) {
            State::Archived
        } else {
            self.freshness
        }
    }
}

/// Every read-back kept in `dir`, in the order of the library: its modules as
/// the table of contents lists them, and environments as they appear. A
/// current read-back takes its environment's prose and Lean as they are now.
fn entries(dir: &Path, report: &Report, made: &[String]) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let path = item?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(mut r) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|label| load(dir, label))
        else {
            continue;
        };
        let env = report
            .environments
            .iter()
            .find(|e| e.record.label.as_deref() == Some(r.label.as_str()));
        let formal = env
            .and_then(|e| e.fingerprints.as_ref())
            .map(|f| f.formal.as_str());
        let (freshness, gone, line) = match (env, formal) {
            (Some(e), Some(formal)) if formal == r.formal => {
                // Prose can change without invalidating a blind formal translation.
                let record = &e.record;
                if (&r.prose, &r.lean, &r.display, &r.module)
                    != (&record.prose, &record.lean, &record.display, &record.module)
                {
                    r.prose = record.prose.clone();
                    r.lean = record.lean.clone();
                    r.display = record.display.clone();
                    r.module = record.module.clone();
                    std::fs::write(&path, serde_json::to_string_pretty(&r)?)?;
                }
                let fresh = if made.contains(&r.label) {
                    State::New
                } else {
                    State::Current
                };
                (fresh, false, record.line)
            }
            (Some(e), Some(_)) => (State::Stale, false, e.record.line),
            (Some(e), None) => (State::Stale, true, e.record.line),
            (None, _) => (State::Stale, true, u32::MAX),
        };
        let rank = if r.module == report.library {
            0
        } else {
            report
                .modules
                .iter()
                .position(|m| m.name == r.module)
                .map_or(usize::MAX, |p| p + 1)
        };
        let key = (rank, r.module.clone(), line, r.label.clone());
        out.push((
            key,
            Entry {
                readback: r,
                freshness,
                gone,
            },
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out.into_iter().map(|(_, e)| e).collect())
}

/// Whether a module is the one named, by its full name or its last parts.
fn in_module(module: &str, name: &str) -> bool {
    module == name || module.ends_with(&format!(".{name}"))
}

/// The read-backs named by labels or a module, or every one; and the labels
/// named that have none.
fn select<'a>(
    entries: &'a [Entry],
    labels: &[String],
    module: Option<&str>,
) -> (Vec<&'a Entry>, Vec<String>) {
    if !labels.is_empty() {
        let chosen = entries
            .iter()
            .filter(|e| labels.iter().any(|l| l == e.label()))
            .collect();
        let missing = labels
            .iter()
            .filter(|l| !entries.iter().any(|e| e.label() == l.as_str()))
            .cloned()
            .collect();
        return (chosen, missing);
    }
    let chosen = entries
        .iter()
        .filter(|e| module.is_none_or(|m| in_module(&e.readback.module, m)))
        .collect();
    (chosen, Vec::new())
}

/// What `--json` says of each read-back: never its text, which is for the
/// person to read first.
fn listing(
    entries: &[Entry],
    reviews: &Reviews,
    address: impl Fn(&str) -> String,
) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|e| {
            json!({
                "label": e.label(),
                "display": e.readback.display,
                "module": e.readback.module,
                "state": e.state(reviews),
                "address": address(e.label()),
            })
        })
        .collect()
}

/// How many read-backs there are in each state, in words.
fn summary(entries: &[Entry], reviews: &Reviews) -> String {
    let count = |s: State| entries.iter().filter(|e| e.state(reviews) == s).count();
    let parts: Vec<String> = [State::New, State::Current, State::Stale, State::Archived]
        .into_iter()
        .map(|s| (count(s), s))
        .filter(|(n, _)| *n > 0)
        .map(|(n, s)| format!("{n} {}", s.as_str()))
        .collect();
    if parts.is_empty() {
        "No read-backs yet.".into()
    } else {
        format!("{} read-backs: {}.", entries.len(), parts.join(", "))
    }
}

/// What `--content` says of each read-back: its text, with the prose and the
/// Lean it is to be compared with.
fn content(entries: &[&Entry], reviews: &Reviews) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|e| {
            let r = &e.readback;
            json!({
                "label": r.label, "display": r.display, "module": r.module,
                "state": e.state(reviews), "formal": r.formal, "agent": r.agent,
                "readback": r.readback, "prose": r.prose, "lean": r.lean,
            })
        })
        .collect()
}

/// `stemma readback --content`: the text of read-backs, when the person asks
/// for it.
fn show_content(
    entries: &[Entry],
    reviews: &Reviews,
    options: &Options,
    json_output: bool,
) -> Result<bool> {
    let (chosen, missing) = select(entries, &options.labels, options.module.as_deref());
    if json_output {
        let readbacks = content(&chosen, reviews);
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "readbacks": readbacks, "missing": missing }))?
        );
    } else {
        for e in &chosen {
            let r = &e.readback;
            crate::ui::heading(format!(
                "{} {}  {}",
                r.display,
                r.label,
                crate::ui::dim(format!("{}, {}", r.module, e.state(reviews).as_str()))
            ));
            println!("{}\n", r.readback);
        }
        for label in &missing {
            crate::ui::warning(format!("There is no read-back of {label}."));
        }
    }
    Ok(missing.is_empty())
}

/// `stemma readback --notes`: the person's marks and notes, when they ask the
/// agent to look at them.
fn show_notes(
    library: &Path,
    entries: &[Entry],
    reviews: &Reviews,
    options: &Options,
    json_output: bool,
) -> Result<bool> {
    let named = !options.labels.is_empty() || options.module.is_some();
    let (chosen, mut missing) = select(entries, &options.labels, options.module.as_deref());
    let mut notes = Vec::new();
    for e in chosen {
        let review = reviews.get(e.label());
        if !named && review.mark == reviews::Mark::Unread && review.note.is_empty() {
            continue;
        }
        notes.push(json!({
            "label": e.label(), "module": e.readback.module,
            "state": e.state(reviews), "mark": review.mark, "note": review.note,
            "outdated": review.outdated(&e.readback.formal),
        }));
    }
    // Notes on read-backs that are no longer kept are still the person's.
    for (label, review) in &reviews.readbacks {
        let wanted = if options.labels.is_empty() {
            options.module.is_none()
        } else {
            options.labels.contains(label)
        };
        let orphan = !entries.iter().any(|e| e.label() == label.as_str());
        if wanted && orphan && (review.mark != reviews::Mark::Unread || !review.note.is_empty()) {
            missing.retain(|l| l != label);
            notes.push(json!({
                "label": label, "module": null, "state": null, "mark": review.mark,
                "note": review.note, "outdated": true,
            }));
        }
    }
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "notes": notes, "missing": missing, "file": reviews::path(library),
            }))?
        );
    } else {
        if notes.is_empty() && missing.is_empty() {
            crate::ui::note("No marks or notes yet.");
        }
        for n in &notes {
            let mut mark = n["mark"].as_str().unwrap_or_default().to_string();
            if n["outdated"] == true {
                mark.push_str(", outdated");
            }
            crate::ui::heading(format!(
                "{}  {}",
                n["label"].as_str().unwrap_or_default(),
                crate::ui::dim(mark)
            ));
            let note = n["note"].as_str().unwrap_or_default();
            if !note.is_empty() {
                println!("{note}\n");
            }
        }
        for label in &missing {
            crate::ui::warning(format!("There is no read-back of {label}."));
        }
    }
    Ok(missing.is_empty())
}

/// A link to the read-back of an environment on the page kept on disk
/// (`index.html#<label>`), when one was made of its Lean as it is now.
pub fn link(library: &Path, env: &Environment) -> Option<String> {
    let label = env.record.label.as_deref()?;
    let formal = &env.fingerprints.as_ref()?.formal;
    let dir = dir_of(library);
    let page = dir.join("index.html");
    let current = load(&dir, label).is_some_and(|r| &r.formal == formal);
    (current && page.is_file()).then(|| format!("file://{}#{label}", page.display()))
}

/// Archives the read-back of an environment a person has just signed, when it
/// was made of the Lean they signed. Read-backs are an aid, so this never
/// stops a signature: when it fails, the read-back stays as it was.
pub fn archive_signed(library: &Path, env: &Environment) {
    if let (Some(label), Some(fp)) = (&env.record.label, &env.fingerprints) {
        let _ = archive_if_made_of(library, label, &fp.formal);
    }
}

/// Archives the read-back of `label` if it was made of the formal fingerprint
/// `formal`, and says whether it did.
fn archive_if_made_of(library: &Path, label: &str, formal: &str) -> Result<bool> {
    match load(&dir_of(library), label) {
        Some(r) if r.formal == formal => {
            let mut reviews = Reviews::load(library)?;
            let change = reviews::Change {
                label: label.to_string(),
                archived: Some(true),
                ..reviews::Change::default()
            };
            reviews.apply(&change, formal)?;
            reviews.save(library)?;
            Ok(true)
        }
        _ => Ok(false),
    }
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
    if options.content || options.notes {
        let entries = entries(&dir, &report, &[])?;
        let reviews = Reviews::load(&library.dir)?;
        return if options.content {
            show_content(&entries, &reviews, &options, json_output)
        } else {
            show_notes(&library.dir, &entries, &reviews, &options, json_output)
        };
    }
    let chosen: Vec<&Environment> = report
        .environments
        .iter()
        .filter(|e| e.fingerprints.is_some())
        .filter(|e| {
            let label = e.record.label.clone().unwrap_or_default();
            if !options.labels.is_empty() {
                options.labels.contains(&label)
            } else if let Some(m) = &options.module {
                in_module(&e.record.module, m)
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
        let current = load(&dir, &label).is_some_and(|r| r.formal == formal);
        if current && !options.force {
            continue;
        }
        let spinner = crate::ui::Spinner::start(format!("Reading back {label}"), json_output);
        let text = translate(
            options.translator,
            options.model.as_deref(),
            &prompt(&formal_side(&report, env), &env.record.decls),
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
        std::fs::write(
            dir.join(format!("{label}.json")),
            serde_json::to_string_pretty(&r)?,
        )?;
        made.push(label);
    }
    let entries = entries(&dir, &report, &made)?;
    let reviews = Reviews::load(&library.dir)?;
    let title = &library.config.library.title;
    // The page on disk can be read, but marks are written only through the
    // page `stemma readback` serves.
    let index = dir.join("index.html");
    std::fs::write(&index, page::render(title, &entries, &reviews, None)?)?;
    if !options.serve {
        if json_output {
            let at = |label: &str| format!("{}#{label}", index.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "made": made, "page": index,
                    "readbacks": listing(&entries, &reviews, at),
                }))?
            );
        } else {
            crate::ui::success(format!(
                "Made {} read-backs {}",
                made.len(),
                crate::ui::dim(index.display())
            ));
            crate::ui::note(summary(&entries, &reviews));
        }
        return Ok(true);
    }
    let (listener, port) = site::bind(options.port)?;
    let url = format!("http://127.0.0.1:{port}/");
    if json_output {
        let at = |label: &str| format!("{url}#{label}");
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "made": made, "page": index, "url": url,
                "readbacks": listing(&entries, &reviews, at),
            }))?
        );
    } else {
        crate::ui::success(format!(
            "Made {} read-backs. Serving them at {}",
            made.len(),
            crate::ui::bold(&url)
        ));
        crate::ui::note(summary(&entries, &reviews));
        crate::ui::note("Ctrl-C to stop.");
    }
    server::Server::new(library.dir.clone(), title.clone(), entries, port).run(listener);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_covers_every_target_declaration() {
        let text = prompt(
            "structure First\ndef Second",
            &["First".into(), "Second".into()],
        );
        assert!(text.contains("EVERY declaration"));
        assert!(text.contains("Target declarations:\nFirst\nSecond"));
        assert!(text.contains("structures, definitions, and instances"));
        assert!(!text.contains("LAST declaration"));
    }

    /// A read-back of `label`, made of the formal fingerprint `formal`.
    fn readback(label: &str, formal: &str) -> Readback {
        Readback {
            label: label.into(),
            display: "Theorem".into(),
            module: "Alg.Even".into(),
            formal: formal.into(),
            prose: "The sum of two even numbers is even.".into(),
            lean: "theorem even_add …".into(),
            readback: "THE TEXT OF THE READ-BACK".into(),
            agent: "claude".into(),
        }
    }

    /// A library's report with one environment per (label, formal fingerprint).
    fn report(envs: &[(&str, &str)]) -> Report {
        let environments: Vec<_> = envs
            .iter()
            .enumerate()
            .map(|(i, (label, formal))| {
                json!({
                    "record": {
                        "name": "theorem", "display": "Theorem", "base": "statement",
                        "label": label, "central": true, "cited": null, "title": null,
                        "of": null, "decls": ["even_add"], "prose": "The prose now.",
                        "lean": "theorem even_add …", "module": "Alg.Even", "line": i + 1,
                    },
                    "state": "proved",
                    "fingerprints": { "version": 1, "prose": "sha256:p", "formal": formal },
                    "closure": [],
                })
            })
            .collect();
        serde_json::from_value(json!({
            "version": 1, "library": "Alg", "rootDocument": false,
            "modules": [{ "name": "Alg.Even", "kind": "document" }],
            "environments": environments, "diagnostics": [],
        }))
        .unwrap()
    }

    /// A library directory with read-backs on disk.
    fn library(name: &str, readbacks: &[Readback]) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stemma-readback-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir_of(&dir)).unwrap();
        for r in readbacks {
            write(&dir, r);
        }
        dir
    }

    fn write(dir: &Path, r: &Readback) {
        let path = dir_of(dir).join(format!("{}.json", r.label));
        std::fs::write(path, serde_json::to_string_pretty(r).unwrap()).unwrap();
    }

    #[test]
    fn states_compare_read_backs_with_the_library() {
        let dir = library(
            "states",
            &[
                readback("current", "sha256:a"),
                readback("made", "sha256:b"),
                readback("changed", "sha256:old"),
                readback("removed", "sha256:d"),
            ],
        );
        let report = report(&[
            ("current", "sha256:a"),
            ("made", "sha256:b"),
            ("changed", "sha256:new"),
        ]);
        let entries = entries(&dir_of(&dir), &report, &["made".into()]).unwrap();
        let reviews = Reviews::default();
        let states: Vec<_> = entries
            .iter()
            .map(|e| (e.label(), e.state(&reviews), e.gone))
            .collect();
        assert_eq!(
            states,
            [
                ("current", State::Current, false),
                ("made", State::New, false),
                ("changed", State::Stale, false),
                ("removed", State::Stale, true),
            ]
        );
        // A current read-back takes the prose as it is now.
        assert_eq!(entries[0].readback.prose, "The prose now.");
        assert_eq!(
            load(&dir_of(&dir), "current").unwrap().prose,
            "The prose now."
        );
    }

    #[test]
    fn the_listing_leaves_the_text_out() {
        let dir = library("listing", &[readback("even-add", "sha256:a")]);
        let entries = entries(&dir_of(&dir), &report(&[("even-add", "sha256:a")]), &[]).unwrap();
        let reviews = Reviews::default();
        let list = listing(&entries, &reviews, |l| {
            format!("http://127.0.0.1:8001/#{l}")
        });
        let text = serde_json::to_string(&list).unwrap();
        assert_eq!(list[0]["label"], "even-add");
        assert_eq!(list[0]["state"], "current");
        assert_eq!(list[0]["address"], "http://127.0.0.1:8001/#even-add");
        assert!(!text.contains("THE TEXT OF THE READ-BACK"));
        assert!(!text.contains("prose") && !text.contains("lean"));
        // The text is there when the person asks for it.
        let (chosen, missing) = select(&entries, &["even-add".into(), "other".into()], None);
        assert_eq!(missing, ["other"]);
        let content = content(&chosen, &reviews);
        assert_eq!(content[0]["readback"], "THE TEXT OF THE READ-BACK");
        assert_eq!(content[0]["prose"], "The prose now.");
    }

    #[test]
    fn marks_survive_a_regeneration_and_age_with_the_lean() {
        let dir = library("marks", &[readback("even-add", "sha256:a")]);
        let mut reviews = Reviews::default();
        let approve = reviews::Change {
            label: "even-add".into(),
            mark: Some(reviews::Mark::Approved),
            note: Some("Fine.".into()),
            ..reviews::Change::default()
        };
        reviews.apply(&approve, "sha256:a").unwrap();
        reviews.save(&dir).unwrap();
        // Made again from the same Lean: the mark still holds.
        let mut again = readback("even-add", "sha256:a");
        again.readback = "Another wording.".into();
        write(&dir, &again);
        let reviews = Reviews::load(&dir).unwrap();
        let report_a = report(&[("even-add", "sha256:a")]);
        let entries_a = entries(&dir_of(&dir), &report_a, &["even-add".into()]).unwrap();
        let formal = &entries_a[0].readback.formal;
        assert!(!reviews.get("even-add").outdated(formal));
        assert_eq!(reviews.get("even-add").mark, reviews::Mark::Approved);
        let html = page::render("T", &entries_a, &reviews, None).unwrap();
        assert!(html.contains("data-mark=\"approved\" data-outdated=\"false\""));
        // Made from other Lean: the mark is outdated.
        write(&dir, &readback("even-add", "sha256:b"));
        let report_b = report(&[("even-add", "sha256:b")]);
        let entries_b = entries(&dir_of(&dir), &report_b, &[]).unwrap();
        let formal = &entries_b[0].readback.formal;
        assert!(reviews.get("even-add").outdated(formal));
        let html = page::render("T", &entries_b, &reviews, None).unwrap();
        assert!(html.contains("data-mark=\"approved\" data-outdated=\"true\""));
    }

    #[test]
    fn signing_archives_the_read_back_of_what_was_signed() {
        let dir = library(
            "archive",
            &[
                readback("even-add", "sha256:a"),
                readback("odd", "sha256:old"),
            ],
        );
        let signed = report(&[("even-add", "sha256:a"), ("odd", "sha256:new")]);
        for env in &signed.environments {
            archive_signed(&dir, env);
        }
        let reviews = Reviews::load(&dir).unwrap();
        assert!(reviews.get("even-add").is_archived("sha256:a"));
        // A read-back of other Lean than what was signed stays as it was.
        assert!(!reviews.readbacks.contains_key("odd"));
        let entries = entries(&dir_of(&dir), &signed, &[]).unwrap();
        assert_eq!(entries[0].state(&reviews), State::Archived);
        assert_eq!(entries[1].state(&reviews), State::Stale);
        // Links point at a current read-back's place on the page.
        std::fs::write(dir_of(&dir).join("index.html"), "").unwrap();
        let link = link(&dir, &signed.environments[0]).unwrap();
        assert!(link.starts_with("file://") && link.ends_with("index.html#even-add"));
        assert_eq!(super::link(&dir, &signed.environments[1]), None);
        // Without read-backs, signing leaves no trace.
        let empty = library("archive-none", &[]);
        archive_signed(&empty, &signed.environments[0]);
        assert!(!reviews::path(&empty).exists());
    }

    #[test]
    fn the_page_escapes_html() {
        let mut r = readback("x", "sha256:a");
        r.prose = "a < b".into();
        r.readback = "<script>alert(1)</script>".into();
        let entry = Entry {
            readback: r,
            freshness: State::Current,
            gone: false,
        };
        let html = page::render("T & U", &[entry], &Reviews::default(), None).unwrap();
        assert!(html.contains("a &lt; b") && html.contains("&lt;script&gt;alert(1)"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("T &amp; U"));
        // A copy without a token cannot write.
        assert!(!html.contains("stemma-token\" content"));
        assert!(html.contains("id=\"x\""));
    }
}
