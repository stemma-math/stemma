//! `stemma init`: creates a library, asking what it needs when run in a
//! terminal.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::commands::{LEAN_TOOLCHAIN, STEMMA_GIT, mathlib_rev};
use crate::{agents_md, library, templates, ui};

/// Options of `stemma init`. What is `None` is asked in a terminal, and takes
/// its default otherwise.
pub struct Options {
    pub dir: Option<PathBuf>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub mathlib: Option<bool>,
    pub stemma_lean: Option<PathBuf>,
    pub git: Option<bool>,
    pub update: Option<bool>,
    pub commit: Option<bool>,
    pub remote: Option<String>,
    pub push: Option<bool>,
    /// Members besides the person creating the library, by forge account.
    pub members: Vec<String>,
    /// Take every default without asking.
    pub yes: bool,
}

/// The library name a directory suggests: `group-theory` gives `GroupTheory`.
fn name_from_dir(dir: &Path) -> String {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let base = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Library");
    let name: String = base
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or(String::new(), |c| {
                c.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect();
    if name.is_empty() {
        "Library".into()
    } else {
        name
    }
}

fn valid_name(name: &str) -> bool {
    library::valid_module(name, &format!("{name}.X"))
}

/// Asks the questions `options` leaves open, in a terminal.
struct Asker {
    interactive: bool,
}

impl Asker {
    fn text(&self, given: Option<String>, prompt: &str, default: &str) -> Result<String> {
        if let Some(v) = given {
            return Ok(v);
        }
        if !self.interactive {
            return Ok(default.to_string());
        }
        Ok(cliclack::input(prompt).default_input(default).interact()?)
    }

    /// An optional answer, which may be left empty.
    fn optional(&self, given: Option<String>, prompt: &str, example: &str) -> Result<String> {
        if let Some(v) = given {
            return Ok(v);
        }
        if !self.interactive {
            return Ok(String::new());
        }
        Ok(cliclack::input(prompt)
            .placeholder(example)
            .required(false)
            .interact()?)
    }

    fn name(&self, given: Option<String>, default: &str) -> Result<String> {
        if let Some(v) = given {
            return Ok(v);
        }
        if !self.interactive {
            return Ok(default.to_string());
        }
        Ok(cliclack::input("The library's Lean name")
            .default_input(default)
            .validate(|s: &String| {
                if valid_name(s) {
                    Ok(())
                } else {
                    Err("a Lean name: letters, digits and _, starting with a letter")
                }
            })
            .interact()?)
    }

    /// A yes-or-no question; `batch` is the answer when nobody can be asked.
    fn yes_no(
        &self,
        given: Option<bool>,
        prompt: &str,
        default: bool,
        batch: bool,
    ) -> Result<bool> {
        if let Some(v) = given {
            return Ok(v);
        }
        if !self.interactive {
            return Ok(batch);
        }
        Ok(cliclack::confirm(prompt)
            .initial_value(default)
            .interact()?)
    }
}

/// Runs a command in `dir`, returning its combined output when it fails.
fn run(dir: &Path, program: &str, args: &[&str]) -> std::result::Result<(), String> {
    match Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )),
        Err(e) => Err(format!("could not run `{program}`: {e}")),
    }
}

/// A step's outcome, said in the style of the session.
fn report(interactive: bool, ok: bool, text: &str) {
    match (interactive, ok) {
        (true, true) => drop(cliclack::log::success(text)),
        (true, false) => drop(cliclack::log::warning(text)),
        (false, true) => ui::success(text),
        (false, false) => ui::warning(text),
    }
}

/// A long step, with a spinner.
fn step<T>(interactive: bool, json_output: bool, message: &str, f: impl FnOnce() -> T) -> T {
    if interactive {
        let spinner = cliclack::spinner();
        spinner.start(message);
        let out = f();
        spinner.clear();
        out
    } else {
        let spinner = ui::Spinner::start(message, json_output);
        let out = f();
        spinner.stop();
        out
    }
}

/// `stemma init`.
pub fn init(options: Options, json_output: bool) -> Result<()> {
    let interactive = !options.yes
        && !json_output
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    let ask = Asker { interactive };
    if interactive {
        cliclack::intro(ui::bold(" stemma init "))?;
    }

    let dir = match options.dir {
        Some(d) => d,
        None => PathBuf::from(ask.text(None, "Where should the library live?", ".")?),
    };
    if dir.join("stemma.toml").exists() {
        bail!("{} is already a Stemma library", dir.display());
    }
    let name = ask.name(options.name, &name_from_dir(&dir))?;
    if !valid_name(&name) {
        bail!("'{name}' is not a valid Lean name for a library");
    }
    let title = ask.text(options.title, "Its title", &name)?;
    // The person creating the library is its first member, so that someone can
    // approve the changes the policy reserves to maintainers.
    let creator = crate::agent::person();
    let mut members = vec![ask.text(None, "Your account on the forge", &creator)?];
    let others = if options.members.is_empty() {
        ask.optional(None, "Other members' accounts, separated by commas", "none")?
            .split(',')
            .map(|m| m.trim().to_string())
            .collect()
    } else {
        options.members
    };
    for m in others {
        if !m.is_empty() && !members.contains(&m) {
            members.push(m);
        }
    }
    let mathlib = ask.yes_no(options.mathlib, "Build on Mathlib?", true, true)?;
    let in_repo = dir.join(".git").exists();
    let git = !in_repo && ask.yes_no(options.git, "Create a git repository?", true, true)?;
    let update = ask.yes_no(
        options.update,
        if mathlib {
            "Download the dependencies now? (Mathlib's cache is about 5 GB)"
        } else {
            "Download the dependencies now?"
        },
        true,
        false,
    )?;
    let commit =
        (git || in_repo) && ask.yes_no(options.commit, "Commit the new library?", true, false)?;
    let remote = if git || in_repo {
        let url = ask.optional(
            options.remote,
            "A remote to add as origin, such as git@github.com:group/library.git",
            "none",
        )?;
        (!url.trim().is_empty()).then(|| url.trim().to_string())
    } else {
        None
    };
    let push =
        commit && remote.is_some() && ask.yes_no(options.push, "Push main to it?", false, false)?;

    // The files.
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
        "mathlib": mathlib,
        "mathlib_rev": mathlib_rev(),
        "members": members,
    });
    for sub in [name.as_str(), ".github/workflows"] {
        std::fs::create_dir_all(dir.join(sub))
            .with_context(|| format!("creating {}", dir.display()))?;
    }
    let files = [
        ("stemma.toml", "library/stemma.toml"),
        ("lakefile.toml", "library/lakefile.toml"),
        (&format!("{name}.lean") as &str, "library/root.lean"),
        ("README.md", "library/README.md"),
        (".gitignore", "library/gitignore"),
        (".github/workflows/stemma.yml", "library/github/stemma.yml"),
    ];
    let mut written = Vec::new();
    for (file, template) in files {
        std::fs::write(dir.join(file), templates::render(template, &context)?)?;
        written.push(file.to_string());
    }
    std::fs::write(dir.join("lean-toolchain"), LEAN_TOOLCHAIN)?;
    std::fs::write(dir.join("AGENTS.md"), agents_md::initial(&title))?;
    written.extend(["lean-toolchain".to_string(), "AGENTS.md".to_string()]);
    report(
        interactive && !json_output,
        true,
        &format!("Created the library {name}."),
    );

    // Git, the dependencies, the first commit, the remote.
    let mut done = json!({});
    if git {
        let ok = run(&dir, "git", &["init", "--quiet", "--initial-branch=main"]).is_ok();
        done["git"] = json!(ok);
        if !json_output {
            report(
                interactive,
                ok,
                if ok {
                    "Created a git repository."
                } else {
                    "Could not create a git repository."
                },
            );
        }
    }
    if update {
        let result = step(
            interactive,
            json_output,
            "Downloading the dependencies…",
            || {
                run(&dir, "lake", &["update"]).and_then(|()| {
                    if mathlib {
                        run(&dir, "lake", &["exe", "cache", "get"])
                    } else {
                        Ok(())
                    }
                })
            },
        );
        done["update"] = json!(result.is_ok());
        if !json_output {
            match &result {
                Ok(()) => report(interactive, true, "Downloaded the dependencies."),
                Err(e) => report(
                    interactive,
                    false,
                    &format!(
                        "Could not download the dependencies; run `lake update` later.\n{}",
                        e.trim()
                    ),
                ),
            }
        }
    }
    if commit {
        let message = format!("Create the library {name}");
        let result = run(&dir, "git", &["add", "--all"])
            .and_then(|()| run(&dir, "git", &["commit", "--quiet", "-m", &message]));
        done["commit"] = json!(result.is_ok());
        if !json_output {
            report(
                interactive,
                result.is_ok(),
                if result.is_ok() {
                    "Committed the new library."
                } else {
                    "Could not commit the new library (is git's user.name set?)."
                },
            );
        }
    }
    if let Some(url) = &remote {
        let added = run(&dir, "git", &["remote", "add", "origin", url]).is_ok();
        done["remote"] = json!(added);
        if !json_output {
            report(
                interactive,
                added,
                &if added {
                    format!("Added the remote {url}.")
                } else {
                    format!("Could not add the remote {url}.")
                },
            );
        }
        if push && added {
            let pushed = step(interactive, json_output, "Pushing main…", || {
                run(
                    &dir,
                    "git",
                    &["push", "--quiet", "--set-upstream", "origin", "main"],
                )
            });
            done["push"] = json!(pushed.is_ok());
            if !json_output {
                report(
                    interactive,
                    pushed.is_ok(),
                    if pushed.is_ok() {
                        "Pushed main."
                    } else {
                        "Could not push main."
                    },
                );
            }
        }
    }

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "library": name, "dir": dir, "files": written, "done": done,
            }))?
        );
    } else {
        let next = if dir == Path::new(".") {
            "Next: `stemma claude/codex` to start working.".to_string()
        } else {
            format!(
                "Next: cd {} and `stemma claude/codex` to start working.",
                dir.display()
            )
        };
        if interactive {
            cliclack::outro(next)?;
        } else {
            ui::note(next);
        }
    }
    Ok(())
}
