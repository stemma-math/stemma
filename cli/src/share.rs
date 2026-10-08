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
    /// Whether the work was on `main`, and moved to `branch`.
    moved_from_main: bool,
    /// Files in conflict with `main`, when bringing it in failed.
    conflicts: Vec<String>,
    checks: Option<Checks>,
    pushed: bool,
    pull_request: Option<String>,
    /// Central environments not signed yet (nothing waits for them).
    unsigned: Vec<String>,
    /// What the change lacks that the person can supply with `stemma sign`.
    needs_you: Vec<String>,
    /// What the change lacks that only someone else can supply.
    needs_others: Vec<String>,
}

/// Opens the pull request from `branch` to `main`, or finds the open one.
fn pull_request(dir: &Path, branch: &str) -> Option<String> {
    let forge = crate::forge::current();
    if let Ok(Some(open)) = crate::forge::open_pull_request_from(forge.as_ref(), dir, branch) {
        return Some(open.url);
    }
    let title = git_ok(dir, &["log", "-1", "--format=%s"]).ok()?;
    let new = crate::forge::NewPullRequest {
        head: branch,
        base: "main",
        title: &title,
        body: "",
    };
    forge.open_pull_request(dir, &new).ok().map(|pr| pr.url)
}

/// Shares the working branch: checks, brings `main` in, checks again, pushes,
/// and opens or updates the pull request.
fn share_branch(library: &Library, anyway: bool, json_output: bool) -> Result<Outcome> {
    let dir = &library.dir;
    let Some(mut branch) = crate::branches::current(dir) else {
        bail!("share from a branch: HEAD is on none");
    };
    // Work done on `main` moves to a working branch of its own: nothing is
    // pushed to `main`, and nothing is lost.
    let mut moved_from_main = false;
    if branch == "main" {
        branch = crate::branches::move_to_new(dir, &format!("work/{}", crate::agent::person()))?;
        moved_from_main = true;
    }
    if !git_ok(dir, &["status", "--porcelain"])?.is_empty() {
        if moved_from_main {
            bail!(
                "your work was on `main`, and is now on the branch {branch}; it has \
                 uncommitted changes: commit them, then share again"
            );
        }
        bail!("there are uncommitted changes: commit them before sharing");
    }
    let mut outcome = Outcome {
        branch: branch.clone(),
        moved_from_main,
        conflicts: Vec::new(),
        checks: None,
        pushed: false,
        pull_request: None,
        unsigned: Vec::new(),
        needs_you: Vec::new(),
        needs_others: Vec::new(),
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
            if env.signature(dir) == Some(Signature::Unsigned) {
                outcome
                    .unsigned
                    .push(env.record.label.clone().unwrap_or_default());
            }
        }
    }
    // What the pull request will lack: what the person can supply stops the
    // sharing, so that they sign before the checks fail; what only others can
    // supply is reported once the pull request is open.
    let verdict = crate::verify::evaluate(library, "origin/main", checks.report.as_ref())?;
    let me = crate::keys::signing_key(dir).and_then(|key| {
        verdict
            .base_config
            .member_with_key(&key)
            .map(str::to_string)
    });
    for missing in &verdict.missing {
        let mine = me
            .as_deref()
            .is_some_and(|m| missing.can_supply(&verdict.base_config, m));
        if mine {
            outcome.needs_you.push(missing.describe());
        } else {
            outcome.needs_others.push(missing.describe());
        }
    }
    outcome.checks = Some(checks);
    if !outcome.needs_you.is_empty() && !anyway {
        return Ok(outcome);
    }
    git_ok(
        dir,
        &["push", "--quiet", "--set-upstream", "origin", &branch],
    )?;
    outcome.pushed = true;
    outcome.pull_request = pull_request(dir, &branch);
    Ok(outcome)
}

/// `stemma share`.
pub fn share(anyway: bool, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let o = share_branch(&library, anyway, json_output)?;
    let checks_ok = o.checks.as_ref().is_some_and(Checks::ok);
    let ok = o.conflicts.is_empty() && checks_ok && o.pushed;
    if json_output {
        let (problems, build) = o
            .checks
            .as_ref()
            .map_or((vec![], None), |c| (c.problems.clone(), c.build.clone()));
        let value = json!({
            "ok": ok, "branch": o.branch, "moved_from_main": o.moved_from_main,
            "conflicts": o.conflicts,
            "problems": problems, "build": build, "pushed": o.pushed,
            "pull_request": o.pull_request,
            "needs_you": o.needs_you, "needs_others": o.needs_others,
            "unsigned": o.unsigned,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(ok);
    }
    if o.moved_from_main {
        ui::note(format!(
            "Your work was on `main`: it is now on the branch {}.",
            ui::bold(&o.branch)
        ));
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
    if !o.pushed {
        ui::warning("Not shared yet: the pull request would lack what you can give it.");
        for n in &o.needs_you {
            ui::error(n);
        }
        ui::note(
            "Run `stemma sign` in your own terminal, then share again (`--anyway` shares now).",
        );
        return Ok(false);
    }
    match &o.pull_request {
        Some(url) => ui::success(format!("Shared {}: {url}", ui::bold(&o.branch))),
        None => ui::warning(format!(
            "Pushed {}. Open a pull request from it to `main` (`gh` could not).",
            o.branch
        )),
    }
    for n in o.needs_you.iter().chain(&o.needs_others) {
        ui::warning(n);
    }
    if !o.needs_others.is_empty() {
        ui::note("It needs someone else's signature or approval before it can be merged.");
    }
    if !o.unsigned.is_empty() {
        ui::note(format!(
            "Central and not signed yet: {}.",
            o.unsigned.join(", ")
        ));
    }
    Ok(ok)
}
