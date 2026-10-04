//! `stemma share`: bringing `main` in, and opening or updating the pull request.

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::commands::{self, Checks};
use crate::library::Library;
use crate::report::Signature;
use crate::ui;

/// Runs git in the library.
fn git(dir: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running `git {}`", args.join(" ")))
}

/// Runs git, failing with its error when it fails.
fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git(dir, args)?;
    if !out.status.success() {
        bail!(
            "`git {}` failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What sharing found, for the person and for agents.
struct Outcome {
    branch: String,
    /// Files in conflict with `main`, when bringing it in failed.
    conflicts: Vec<String>,
    checks: Option<Checks>,
    pushed: bool,
    pull_request: Option<String>,
    /// Central environments the pull request leaves unsigned or stale.
    unsigned: Vec<String>,
    stale: Vec<String>,
}

/// Opens the pull request from `branch` to `main`, or finds the open one.
fn pull_request(dir: &Path, branch: &str) -> Option<String> {
    let view = Command::new("gh")
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "url,state",
            "--jq",
            "select(.state == \"OPEN\") | .url",
        ])
        .current_dir(dir)
        .output()
        .ok()?;
    let existing = String::from_utf8_lossy(&view.stdout).trim().to_string();
    if view.status.success() && !existing.is_empty() {
        return Some(existing);
    }
    let create = Command::new("gh")
        .args(["pr", "create", "--base", "main", "--head", branch, "--fill"])
        .current_dir(dir)
        .output()
        .ok()?;
    create
        .status
        .success()
        .then(|| String::from_utf8_lossy(&create.stdout).trim().to_string())
}

/// Shares the working branch: checks, brings `main` in, checks again, pushes,
/// and opens or updates the pull request.
fn share_branch(library: &Library, json_output: bool) -> Result<Outcome> {
    let dir = &library.dir;
    let branch = git_ok(dir, &["branch", "--show-current"])?;
    if branch.is_empty() || branch == "main" {
        bail!("share from a working branch, not from `main`");
    }
    if !git_ok(dir, &["status", "--porcelain"])?.is_empty() {
        bail!("there are uncommitted changes: commit them before sharing");
    }
    let mut outcome = Outcome {
        branch: branch.clone(),
        conflicts: Vec::new(),
        checks: None,
        pushed: false,
        pull_request: None,
        unsigned: Vec::new(),
        stale: Vec::new(),
    };
    git_ok(dir, &["fetch", "--quiet", "origin"])?;
    let has_main = git(dir, &["rev-parse", "--verify", "--quiet", "origin/main"])?
        .status
        .success();
    if !has_main {
        bail!(
            "the remote has no `main` yet: a person publishes the library first, with \
             `git push origin main`"
        );
    }
    let merge = git(dir, &["merge", "--no-edit", "origin/main"])?;
    if !merge.status.success() {
        let conflicts = git_ok(dir, &["diff", "--name-only", "--diff-filter=U"])?;
        outcome.conflicts = conflicts.lines().map(str::to_string).collect();
        if outcome.conflicts.is_empty() {
            bail!(
                "bringing in `main` failed:\n{}",
                String::from_utf8_lossy(&merge.stderr)
            );
        }
        return Ok(outcome);
    }
    let checks = commands::run_checks(library, json_output)?;
    if !checks.ok() {
        outcome.checks = Some(checks);
        return Ok(outcome);
    }
    if let Some(report) = &checks.report {
        for env in &report.environments {
            let label = env.record.label.clone().unwrap_or_default();
            match env.signature(dir) {
                Some(Signature::Unsigned) => outcome.unsigned.push(label),
                Some(Signature::Stale) => outcome.stale.push(label),
                _ => {}
            }
        }
    }
    outcome.checks = Some(checks);
    git_ok(
        dir,
        &["push", "--quiet", "--set-upstream", "origin", &branch],
    )?;
    outcome.pushed = true;
    outcome.pull_request = pull_request(dir, &branch);
    Ok(outcome)
}

/// `stemma share`.
pub fn share(json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let o = share_branch(&library, json_output)?;
    let checks_ok = o.checks.as_ref().is_some_and(Checks::ok);
    let ok = o.conflicts.is_empty() && checks_ok && o.pushed;
    if json_output {
        let (problems, build) = o
            .checks
            .as_ref()
            .map_or((vec![], None), |c| (c.problems.clone(), c.build.clone()));
        let value = json!({
            "ok": ok, "branch": o.branch, "conflicts": o.conflicts,
            "problems": problems, "build": build, "pushed": o.pushed,
            "pull_request": o.pull_request,
            "needs_signatures": o.stale, "unsigned": o.unsigned,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(ok);
    }
    if !o.conflicts.is_empty() {
        ui::error("Bringing in `main` left conflicts in:");
        for f in &o.conflicts {
            println!("    {}", ui::bold(f));
        }
        ui::note("Resolve them, commit, and share again.");
        return Ok(false);
    }
    if let Some(checks) = &o.checks
        && !checks.ok()
    {
        if let Some(build) = &checks.build {
            commands::show_build_errors(build);
        }
        for p in &checks.problems {
            ui::error(p);
        }
        return Ok(false);
    }
    match &o.pull_request {
        Some(url) => ui::success(format!("Shared {}: {url}", ui::bold(&o.branch))),
        None => ui::warning(format!(
            "Pushed {}. Open a pull request from it to `main` (`gh` could not).",
            o.branch
        )),
    }
    if !o.stale.is_empty() {
        ui::warning(format!(
            "It needs signatures before it can be merged: {}.",
            o.stale.join(", ")
        ));
        ui::note("A signer runs `stemma sign` in their own terminal.");
    }
    if !o.unsigned.is_empty() {
        ui::note(format!(
            "Central and not signed yet: {}.",
            o.unsigned.join(", ")
        ));
    }
    Ok(ok)
}
