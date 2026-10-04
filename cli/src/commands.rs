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

/// Where Stemma's Lean package is published.
pub const STEMMA_GIT: &str = "https://github.com/stemma-math/stemma";

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
    let listed = library.table_of_contents()?;
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
            "AGENTS.md lacks the current block stemma keeps in it; `stemma claude` or \
             `stemma codex` writes it."
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
    let spinner = ui::Spinner::start("Building the library", json_output);
    let build = lake::build(library);
    spinner.stop();
    let build = build?;
    if !build.success {
        return Ok(Err(lake::problems(&build.output)));
    }
    let spinner = ui::Spinner::start("Reading its state", json_output);
    let report = lake::extract(library);
    spinner.stop();
    Ok(Ok(report?))
}

/// Says that the library does not build, with Lake's errors.
pub fn show_build_errors(output: &str) {
    ui::error("The library does not build:");
    println!("{output}");
}

/// The outcome of every check of the specification.
pub struct Checks {
    pub problems: Vec<String>,
    /// Lake's errors, when the library does not build.
    pub build: Option<String>,
    /// The library's report, when it builds.
    pub report: Option<Report>,
}

impl Checks {
    pub fn ok(&self) -> bool {
        self.problems.is_empty() && self.build.is_none()
    }
}

/// Runs every check of the specification on a library.
pub fn run_checks(library: &Library, json_output: bool) -> Result<Checks> {
    let mut problems = disk_diagnostics(library)?;
    let version = env!("CARGO_PKG_VERSION");
    if library.config.library.stemma != version {
        problems.push(format!(
            "The library uses stemma {}, but this is stemma {version}.",
            library.config.library.stemma
        ));
    }
    let (build, report) = match build_and_extract(library, json_output)? {
        Ok(report) => {
            for d in &report.diagnostics {
                problems.push(match (&d.module, d.line) {
                    (Some(m), Some(l)) => format!("{m}:{l}: {}", d.message),
                    (Some(m), None) => format!("{m}: {}", d.message),
                    _ => d.message.clone(),
                });
            }
            (None, Some(report))
        }
        Err(output) => (Some(output), None),
    };
    problems.dedup();
    Ok(Checks {
        problems,
        build,
        report,
    })
}

/// `stemma check`: every check of the specification. Fails when one does.
pub fn check(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let Checks {
        problems, build, ..
    } = run_checks(&library, json_output)?;
    let build_errors = build;
    let ok = problems.is_empty() && build_errors.is_none();
    emit(
        json_output,
        &json!({ "ok": ok, "build": build_errors, "problems": problems }),
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
        let value = json!({ "environments": environments, "diagnostics": report.diagnostics });
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
    for d in &report.diagnostics {
        ui::error(&d.message);
    }
    Ok(true)
}
