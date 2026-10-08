//! The commands of `stemma`.

use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::json;

use crate::lake;
use crate::library::{self, Library};
use crate::report::{Report, Signature};
use crate::templates;
use crate::ui;

/// The Lean toolchain this version of Stemma works with.
pub const LEAN_TOOLCHAIN: &str = include_str!("../../lean/lean-toolchain");

/// The base changes are judged against, unless a command is told otherwise.
pub const DEFAULT_BASE: &str = "origin/main";

/// Where Stemma's Lean package is published.
pub const STEMMA_GIT: &str = "https://github.com/stemma-math/stemma";

/// The library's references, which documents cite.
pub const REFERENCES: &str = "references.bib";

/// The Mathlib release matching the Lean toolchain.
pub fn mathlib_rev() -> &'static str {
    LEAN_TOOLCHAIN
        .trim()
        .rsplit_once(':')
        .map_or("master", |(_, v)| v)
}

/// Prints `value` as JSON when asked to, and `human` otherwise.
fn emit(json_output: bool, value: &impl Serialize, human: impl FnOnce()) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        human();
    }
    Ok(())
}

/// Options of `stemma new`.
pub struct NewOptions {
    pub module: String,
    pub lean: bool,
    pub title: Option<String>,
    pub after: Option<String>,
}

/// `stemma new`: creates a module and adds it to the table of contents.
pub fn new(options: NewOptions, json_output: bool) -> Result<()> {
    let library = Library::find(Path::new("."))?;
    let name = library.name();
    let module = if library::in_library(name, &options.module) {
        options.module.clone()
    } else {
        format!("{name}.{}", options.module)
    };
    if !library::valid_module(name, &module) {
        bail!("'{module}' is not a valid module name");
    }
    let path = library.module_path(&module);
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    let after = options.after.map(|a| {
        if a == "Stemma" || library::in_library(name, &a) {
            a
        } else {
            format!("{name}.{a}")
        }
    });
    let root = std::fs::read_to_string(library.root_path())?;
    let root = library::insert_import(&root, name, &module, after.as_deref())?;
    let title = options
        .title
        .unwrap_or_else(|| module.rsplit('.').next().unwrap_or(&module).to_string());
    let template = if options.lean {
        "module/lean.lean"
    } else {
        "module/document.lean"
    };
    let text = templates::render(template, json!({ "title": title }))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, text)?;
    std::fs::write(library.root_path(), root)?;
    let kind = if options.lean { "lean" } else { "document" };
    let described = if options.lean {
        "Lean module"
    } else {
        "document"
    };
    emit(
        json_output,
        &json!({ "module": module, "kind": kind, "path": path }),
        || {
            ui::success(format!(
                "Created the {described} {} {}",
                ui::bold(&module),
                ui::dim(path.strip_prefix(&library.dir).unwrap_or(&path).display())
            ))
        },
    )
}

/// Problems in the table of contents that only the files on disk show.
fn disk_diagnostics(library: &Library) -> Result<Vec<String>> {
    let on_disk = library.modules_on_disk()?;
    let listed = match library.table_of_contents() {
        Ok(listed) => listed,
        Err(error) => return Ok(vec![format!("{}.lean, {error:#}.", library.name())]),
    };
    let mut out = Vec::new();
    for m in &on_disk {
        if !listed.contains(m) {
            out.push(format!(
                "The module {m} is missing from the table of contents ({}.lean).",
                library.name()
            ));
        }
    }
    if !crate::agents_md::is_current(&library.dir) {
        out.push(
            "AGENTS.md lacks the current block stemma keeps in it; `stemma agent <harness>` \
             or `stemma upgrade` writes it."
                .into(),
        );
    }
    for (i, m) in listed.iter().enumerate() {
        if listed[..i].contains(m) {
            out.push(format!(
                "The module {m} is imported twice by the table of contents."
            ));
        }
    }
    Ok(out)
}

/// Builds the library and extracts its report, or explains why it could not.
pub fn build_and_extract(
    library: &Library,
    json_output: bool,
) -> Result<std::result::Result<Report, String>> {
    Ok(build_and_extract_timed(library, json_output)?.0)
}

/// [`build_and_extract`], with the seconds `lake build` took.
fn build_and_extract_timed(
    library: &Library,
    json_output: bool,
) -> Result<(std::result::Result<Report, String>, f64)> {
    let spinner = ui::Spinner::start("Building the library", json_output);
    let start = std::time::Instant::now();
    let build = lake::build(library);
    let seconds = start.elapsed().as_secs_f64();
    spinner.stop();
    let build = build?;
    if !build.success {
        return Ok((Err(lake::problems(&build.output)), seconds));
    }
    let spinner = ui::Spinner::start("Reading its state", json_output);
    let report = lake::extract(library);
    spinner.stop();
    Ok((Ok(report?), seconds))
}

/// Says that the library does not build, with Lake's errors and hints to fix them.
pub fn show_build_errors(output: &str) {
    ui::error("The library does not build:");
    println!("{output}");
    for hint in build_hints(output) {
        ui::note(hint);
    }
}

/// Hints for errors whose fix Stemma knows. Verso reports a directive name
/// as ambiguous when another package (Mathlib's `lemma` command) declares the
/// same name: the qualified name of Stemma's environment resolves it.
pub fn build_hints(output: &str) -> Vec<String> {
    const START: &str = "directive name `";
    const MIDDLE: &str = "` is ambiguous. Candidates: ";
    let mut hints = Vec::new();
    for line in output.lines() {
        let Some((_, rest)) = line.split_once(START) else {
            continue;
        };
        let Some((name, candidates)) = rest.split_once(MIDDLE) else {
            continue;
        };
        let qualified = format!("Stemma.{name}");
        if candidates.split(", ").any(|c| c.trim() == qualified) {
            let hint = format!(
                "`:::{name}` is ambiguous here, because an imported package (such as Mathlib) \
                 also declares `{name}`: write `:::{qualified}` instead."
            );
            if !hints.contains(&hint) {
                hints.push(hint);
            }
        }
    }
    hints
}

/// The outcome of every check of the specification.
pub struct Checks {
    /// Every problem but the signatures the policy requires.
    pub problems: Vec<String>,
    /// Central environments without a current signature, when the policy
    /// requires one: what a signer can supply, said label by label.
    pub signatures: Vec<String>,
    /// Lake's errors, when the library does not build.
    pub build: Option<String>,
    /// The seconds `lake build` took.
    pub build_seconds: f64,
    /// The library's report, when it builds.
    pub report: Option<Report>,
}

impl Checks {
    pub fn ok(&self) -> bool {
        self.ok_but_signatures() && self.signatures.is_empty()
    }

    /// Whether every check passes but, perhaps, the signatures the policy
    /// requires.
    pub fn ok_but_signatures(&self) -> bool {
        self.problems.is_empty() && self.build.is_none()
    }

    /// The build, as `--json` outputs say it: always an object, so that an
    /// agent never mistakes a build that passed for one that did not run.
    pub fn build_json(&self) -> serde_json::Value {
        build_json(self.build.as_deref(), self.build_seconds)
    }

    /// Every problem, the missing signatures included.
    pub fn all_problems(&self) -> Vec<String> {
        self.problems
            .iter()
            .chain(&self.signatures)
            .cloned()
            .collect()
    }
}

/// The build as `--json` outputs say it: `{"ok": true, "seconds": …}`, or
/// `{"ok": false, "log": "…"}` with Lake's errors.
pub fn build_json(errors: Option<&str>, seconds: f64) -> serde_json::Value {
    match errors {
        None => json!({ "ok": true, "seconds": (seconds * 100.0).round() / 100.0 }),
        Some(log) => json!({ "ok": false, "log": log }),
    }
}

/// The central environments of a report that lack a current signature, with
/// their state, as problems.
pub fn required_signatures(library: &Library, report: &Report) -> Vec<String> {
    let mut out = Vec::new();
    for env in &report.environments {
        let label = env.record.label.as_deref().unwrap_or_default();
        let at = format!("{}:{}", env.record.module, env.record.line);
        match env.signature(&library.dir) {
            Some(Signature::Unsigned) if env.fingerprints.is_none() => {
                out.push(format!(
                    "{at}: {}",
                    crate::verify::unformalized_central(label)
                ));
            }
            Some(Signature::Unsigned) => out.push(format!(
                "{at}: '{label}' is central and unsigned: the policy requires a signature \
                 (`stemma sign`, by a signer)."
            )),
            Some(Signature::Stale) => out.push(format!(
                "{at}: '{label}' is central and its signature is stale: the policy requires a \
                 current one (`stemma sign`, by a signer)."
            )),
            _ => {}
        }
    }
    out
}

/// Runs every check of the specification on a library. The policy is read
/// from `base` when it has one, so that a change cannot turn it off for
/// itself.
pub fn run_checks(library: &Library, base: &str, json_output: bool) -> Result<Checks> {
    let mut problems = disk_diagnostics(library)?;
    let version = env!("CARGO_PKG_VERSION");
    let pinned = &library.config.library.stemma;
    if pinned != version {
        let fix = match crate::upgrade::parse_version(pinned) {
            Some(v) if v < crate::upgrade::this_version() => {
                "run `stemma upgrade` to move it to this version".to_string()
            }
            _ => format!("install stemma {pinned} to work on it"),
        };
        problems.push(format!(
            "The library uses stemma {pinned}, but this is stemma {version}: {fix}."
        ));
    }
    let mut signatures = Vec::new();
    let (built, build_seconds) = build_and_extract_timed(library, json_output)?;
    let (build, report) = match built {
        Ok(report) => {
            for d in &report.diagnostics {
                problems.push(match (&d.module, d.line) {
                    (Some(m), Some(l)) => format!("{m}:{l}: {}", d.message),
                    (Some(m), None) => format!("{m}: {}", d.message),
                    _ => d.message.clone(),
                });
            }
            if crate::config::Config::judging(&library.dir, base)?
                .policy
                .require_signed_central
            {
                signatures = required_signatures(library, &report);
            }
            (None, Some(report))
        }
        Err(output) => (Some(output), None),
    };
    let members: Vec<String> = library.config.members.keys().cloned().collect();
    problems.extend(crate::sources::check(
        &crate::sources::read(&library.dir),
        &members,
        report.as_ref(),
        &std::fs::read_to_string(library.dir.join(REFERENCES)).unwrap_or_default(),
    ));
    problems.dedup();
    Ok(Checks {
        problems,
        signatures,
        build,
        build_seconds,
        report,
    })
}

/// `stemma check`: every check of the specification. Fails when one does.
pub fn check(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let checks = run_checks(&library, DEFAULT_BASE, json_output)?;
    let problems = checks.all_problems();
    let build = checks.build_json();
    let build_errors = checks.build;
    let ok = problems.is_empty() && build_errors.is_none();
    emit(
        json_output,
        &json!({
            "ok": ok, "build": build, "problems": problems,
            "hints": build_errors.as_deref().map(build_hints).unwrap_or_default(),
        }),
        || {
            if let Some(output) = &build_errors {
                show_build_errors(output);
            }
            for p in &problems {
                ui::error(p);
            }
            if ok {
                ui::success("Every check passes.");
            }
        },
    )?;
    Ok(ok)
}

/// `stemma status`: the state of every environment.
pub fn status(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let report = match build_and_extract(&library, json_output)? {
        Ok(report) => report,
        Err(output) => {
            show_build_errors(&output);
            return Ok(false);
        }
    };
    let signatures: Vec<Option<Signature>> = report
        .environments
        .iter()
        .map(|e| e.signature(&library.dir))
        .collect();
    // What still lacks a signature, label by label, and whether the policy
    // (of the base, as `stemma check` reads it) requires it now.
    let config = crate::config::Config::judging(&library.dir, DEFAULT_BASE)?;
    let required = config.policy.require_signed_central;
    let awaiting: Vec<(&str, Signature)> = report
        .environments
        .iter()
        .zip(&signatures)
        .filter_map(|(e, s)| match s {
            Some(state @ (Signature::Unsigned | Signature::Stale)) => {
                Some((e.record.label.as_deref().unwrap_or_default(), *state))
            }
            _ => None,
        })
        .collect();
    let warnings = crate::verify::keyless_warnings(&config);
    let sources = crate::sources::coverage(&crate::sources::read(&library.dir), &report);
    if json_output {
        let environments: Vec<_> = report
            .environments
            .iter()
            .zip(&signatures)
            .map(|(e, s)| {
                json!({
                    "label": e.record.label, "name": e.record.display, "base": e.record.base,
                    "module": e.record.module, "line": e.record.line,
                    "central": e.record.central, "state": e.state, "signature": s,
                })
            })
            .collect();
        let awaiting: Vec<_> = awaiting
            .iter()
            .map(|(label, state)| {
                json!({
                    "label": label, "signature": state,
                    "required": required || *state == Signature::Stale,
                })
            })
            .collect();
        let value = json!({
            "environments": environments, "awaiting_signature": awaiting,
            "warnings": warnings, "diagnostics": report.diagnostics, "sources": sources,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(true);
    }
    let count = |state: &str| {
        report
            .environments
            .iter()
            .filter(|e| e.state.as_deref() == Some(state))
            .count()
    };
    let count_signature =
        |state: Signature| signatures.iter().filter(|s| **s == Some(state)).count();
    ui::heading(&library.config.library.title);
    println!(
        "{}\n",
        ui::dim(format!(
            "{} proved · {} pending · {} cited · {} not formalized · {} signed · {} unsigned · {} stale",
            count("proved"),
            count("pending"),
            count("cited"),
            count("notFormalized"),
            count_signature(Signature::Signed),
            count_signature(Signature::Unsigned),
            count_signature(Signature::Stale),
        ))
    );
    for module in &report.modules {
        let entries: Vec<_> = report
            .environments
            .iter()
            .zip(&signatures)
            .filter(|(e, _)| e.record.module == module.name)
            .collect();
        println!("{} {}", ui::bold(&module.name), ui::dim(&module.kind));
        if entries.is_empty() {
            println!("  {}", ui::dim("no environments"));
        }
        for (e, signature) in entries {
            let r = &e.record;
            let label = r.label.as_deref().unwrap_or("·");
            let central = if r.central { "central" } else { "" };
            let state = match e.state.as_deref() {
                Some("notFormalized") => ui::state("not formalized", 16),
                Some(other) => ui::state(other, 16),
                None => ui::dim(format!(
                    "{:<16}",
                    r.of.as_ref()
                        .map_or(String::new(), |of| format!("proves {of}"))
                )),
            };
            let signature = match signature {
                Some(Signature::Signed) => ui::state("signed", 0),
                Some(Signature::Unsigned) => ui::state("unsigned", 0),
                Some(Signature::Stale) => ui::state("stale", 0),
                None => String::new(),
            };
            println!(
                "  {:<14} {:<24} {} {state} {signature}",
                r.display,
                label,
                ui::dim(format!("{central:<8}"))
            );
        }
    }
    if !awaiting.is_empty() {
        println!();
        let heading = if required {
            "Awaiting a signature (`stemma check` fails until a signer signs them)"
        } else {
            "Awaiting a signature"
        };
        println!("{}", ui::bold(heading));
        let width = awaiting.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        for (label, state) in &awaiting {
            let state = match state {
                Signature::Stale => "stale",
                _ => "unsigned",
            };
            println!("  {label:<width$}  {}", ui::state(state, 0));
        }
    }
    show_sources(&sources);
    for w in &warnings {
        ui::warning(w);
    }
    for d in &report.diagnostics {
        ui::error(&d.message);
    }
    Ok(true)
}

/// The coverage of the sources the library formalizes, by part and by owner.
fn show_sources(sources: &[crate::sources::SourceCoverage]) {
    for source in sources {
        println!();
        let title = source.title.as_deref().unwrap_or(&source.source);
        let phase = match source.phase.as_deref() {
            Some("planning") => " · the plan is being agreed",
            Some("initial") => " · initial phase",
            Some("ended") => " · initial phase ended",
            _ => "",
        };
        println!(
            "{} {}",
            ui::bold(format!("Source: {title}")),
            ui::dim(format!("sources/{}{phase}", source.source))
        );
        println!("  {}", ui::dim(source.coverage.line()));
        let width = source
            .parts
            .iter()
            .map(|p| p.title.chars().count())
            .max()
            .unwrap_or(0);
        for part in &source.parts {
            println!(
                "  {:<width$}  {}",
                part.title,
                ui::dim(part.coverage.line())
            );
        }
        println!("  {}", ui::bold("By owner"));
        for owner in &source.owners {
            println!(
                "  {:<width$}  {}",
                owner.owner.as_deref().unwrap_or("(nobody)"),
                ui::dim(owner.coverage.line())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_at_the_qualified_name_of_an_ambiguous_environment() {
        let output = "error: Alg/Groups.lean:12:3: directive name `lemma` is ambiguous. \
            Candidates: Stemma.lemma, _root_.lemma\n\
            error: Alg/Groups.lean:20:3: directive name `lemma` is ambiguous. \
            Candidates: Stemma.lemma, _root_.lemma\n\
            error: Alg/Other.lean:3:3: directive name `widget` is ambiguous. \
            Candidates: Foo.widget, _root_.widget";
        let hints = build_hints(output);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("`:::Stemma.lemma`"));
        assert!(build_hints("error: unknown directive `lemma`").is_empty());
    }
}
