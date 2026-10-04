//! The commands of `stemma`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use owo_colors::{OwoColorize, Stream::Stdout};
use serde::Serialize;
use serde_json::json;

use crate::lake;
use crate::library::{self, Library};
use crate::report::{Report, Signature};
use crate::templates;

/// The Lean toolchain this version of Stemma works with.
pub const LEAN_TOOLCHAIN: &str = include_str!("../../lean/lean-toolchain");

/// Where Stemma's Lean package is published.
pub const STEMMA_GIT: &str = "https://github.com/stemma-math/stemma";

/// The Mathlib release matching the Lean toolchain.
fn mathlib_rev() -> &'static str {
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

/// Options of `stemma init`.
pub struct InitOptions {
    pub dir: PathBuf,
    pub name: Option<String>,
    pub title: Option<String>,
    pub mathlib: bool,
    pub stemma_lean: Option<PathBuf>,
    pub git: bool,
}

/// The library name a directory suggests: `group-theory` gives `GroupTheory`.
fn name_from_dir(dir: &Path) -> String {
    let base = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Library");
    base.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or(String::new(), |c| {
                c.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect()
}

/// `stemma init`: creates a library.
pub fn init(options: InitOptions, json_output: bool) -> Result<()> {
    let dir = &options.dir;
    if dir.join("stemma.toml").exists() {
        bail!("{} is already a Stemma library", dir.display());
    }
    let name = options.name.unwrap_or_else(|| name_from_dir(dir));
    if !library::valid_module(&name, &format!("{name}.X")) {
        bail!("'{name}' is not a valid Lean name for a library");
    }
    let title = options.title.unwrap_or_else(|| name.clone());
    let stemma_path = match &options.stemma_lean {
        Some(p) => Some(
            p.canonicalize()
                .with_context(|| format!("resolving {}", p.display()))?,
        ),
        None => None,
    };
    let context = json!({
        "name": name,
        "title": title,
        "package": name.to_lowercase(),
        "version": env!("CARGO_PKG_VERSION"),
        "stemma_git": STEMMA_GIT,
        "stemma_path": stemma_path.map(|p| p.display().to_string()),
        "mathlib": options.mathlib,
        "mathlib_rev": mathlib_rev(),
    });
    std::fs::create_dir_all(dir.join(&name))
        .with_context(|| format!("creating {}", dir.display()))?;
    let files = [
        ("stemma.toml", "library/stemma.toml"),
        ("lakefile.toml", "library/lakefile.toml"),
        (&format!("{name}.lean") as &str, "library/root.lean"),
        ("AGENTS.md", "library/AGENTS.md"),
        ("README.md", "library/README.md"),
        (".gitignore", "library/gitignore"),
    ];
    let mut written = Vec::new();
    for (file, template) in files {
        std::fs::write(dir.join(file), templates::render(template, &context)?)?;
        written.push(file.to_string());
    }
    std::fs::write(dir.join("lean-toolchain"), LEAN_TOOLCHAIN)?;
    written.push("lean-toolchain".into());
    let mut git = false;
    if options.git && !dir.join(".git").exists() {
        git = Command::new("git")
            .args(["init", "--quiet", "--initial-branch=main"])
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success());
    }
    emit(
        json_output,
        &json!({ "library": name, "dir": dir, "files": written, "git": git }),
        || {
            println!(
                "Created the library {} in {}.",
                name.if_supports_color(Stdout, |t| t.bold()),
                dir.display()
            );
            println!("Next: `stemma new <Module>` to add a document, then `stemma check`.");
        },
    )
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
            println!(
                "Created the {described} {} ({}).",
                module.if_supports_color(Stdout, |t| t.bold()),
                path.strip_prefix(&library.dir).unwrap_or(&path).display()
            )
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
fn build_and_extract(library: &Library) -> Result<std::result::Result<Report, String>> {
    let build = lake::build(library)?;
    if !build.success {
        return Ok(Err(lake::problems(&build.output)));
    }
    Ok(Ok(lake::extract(library)?))
}

/// `stemma check`: every check of the specification. Fails when one does.
pub fn check(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let mut problems = disk_diagnostics(&library)?;
    let version = env!("CARGO_PKG_VERSION");
    if library.config.library.stemma != version {
        problems.push(format!(
            "The library uses stemma {}, but this is stemma {version}.",
            library.config.library.stemma
        ));
    }
    let build_errors = match build_and_extract(&library)? {
        Ok(report) => {
            for d in report.diagnostics {
                problems.push(match (d.module, d.line) {
                    (Some(m), Some(l)) => format!("{m}:{l}: {}", d.message),
                    (Some(m), None) => format!("{m}: {}", d.message),
                    _ => d.message,
                });
            }
            None
        }
        Err(output) => Some(output),
    };
    problems.dedup();
    let ok = problems.is_empty() && build_errors.is_none();
    emit(
        json_output,
        &json!({ "ok": ok, "build": build_errors, "problems": problems }),
        || {
            if let Some(output) = &build_errors {
                println!(
                    "{}",
                    "The library does not build:".if_supports_color(Stdout, |t| t.red())
                );
                println!("{output}");
            }
            for p in &problems {
                println!("{} {p}", "error:".if_supports_color(Stdout, |t| t.red()));
            }
            if ok {
                println!(
                    "{}",
                    "Every check passes.".if_supports_color(Stdout, |t| t.green())
                );
            }
        },
    )?;
    Ok(ok)
}

/// `stemma status`: the state of every environment.
pub fn status(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let report = match build_and_extract(&library)? {
        Ok(report) => report,
        Err(output) => {
            println!("The library does not build:\n{output}");
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
    println!(
        "{}\n",
        library
            .config
            .library
            .title
            .if_supports_color(Stdout, |t| t.bold())
    );
    for module in &report.modules {
        let entries: Vec<_> = report
            .environments
            .iter()
            .zip(&signatures)
            .filter(|(e, _)| e.record.module == module.name)
            .collect();
        println!(
            "{} {}",
            module.name.if_supports_color(Stdout, |t| t.bold()),
            format!("({})", module.kind).if_supports_color(Stdout, |t| t.dimmed())
        );
        for (e, signature) in entries {
            let r = &e.record;
            let label = r.label.as_deref().unwrap_or("-");
            let central = if r.central { "central" } else { "" };
            let state = match e.state.as_deref() {
                Some("notFormalized") => "not formalized".to_string(),
                Some(other) => other.to_string(),
                None => {
                    r.of.as_ref()
                        .map_or(String::new(), |of| format!("proof of {of}"))
                }
            };
            let state = format!("{state:<16}");
            let state = match e.state.as_deref() {
                Some("proved") => state.if_supports_color(Stdout, |t| t.green()).to_string(),
                Some("pending") => state.if_supports_color(Stdout, |t| t.yellow()).to_string(),
                Some("cited") => state.if_supports_color(Stdout, |t| t.cyan()).to_string(),
                Some("notFormalized") => state.if_supports_color(Stdout, |t| t.red()).to_string(),
                _ => state,
            };
            let signature = match signature {
                Some(Signature::Signed) => "signed"
                    .if_supports_color(Stdout, |t| t.green())
                    .to_string(),
                Some(Signature::Unsigned) => "unsigned"
                    .if_supports_color(Stdout, |t| t.yellow())
                    .to_string(),
                Some(Signature::Stale) => {
                    "stale".if_supports_color(Stdout, |t| t.red()).to_string()
                }
                None => String::new(),
            };
            println!(
                "  {:<14} {:<24} {:<8} {state} {signature}",
                r.display, label, central
            );
        }
    }
    for d in &report.diagnostics {
        println!(
            "{} {}",
            "error:".if_supports_color(Stdout, |t| t.red()),
            d.message
        );
    }
    Ok(true)
}
