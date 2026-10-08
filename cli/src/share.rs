//! `stemma share`: shares a working branch through its share branch.
//!
//! Each working branch `work/<x>` is shared through `share/<x>`, in the group's
//! repository, and the pull request comes from `share/<x>`. **The invariant:**
//! `share/<x>` only ever points at a commit of `work/<x>`. Nobody commits on
//! it: sharing moves it forward (fast-forward) to HEAD, or to the commit
//! chosen. Everything sharing needs (bringing `main` in, signatures,
//! approvals) happens on `work/<x>`, which stays free while the pull request
//! is open.
//!
//! The state is derived from git each time, from the remote `share/<x>` (never
//! only a local copy, which another machine may have left behind) and from
//! `work/<x>`:
//!
//! - when the remote `share/<x>` is an ancestor of `work/<x>`, it moves forward;
//! - when it has commits `work/<x>` lacks, and all of them come from the earlier
//!   history of `work/<x>` (a rebase, an amend), it is replaced, after asking,
//!   with `--force-with-lease`;
//! - otherwise it has foreign commits (the forge's "Update branch", an accepted
//!   suggestion, a push from elsewhere): they are shown, and brought into
//!   `work/<x>` or, with explicit confirmation, discarded. Never silently.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;

use crate::branches::{self, MAIN, ORIGIN, commit_of, git, git_ok, is_ancestor};
use crate::commands::{self, Checks};
use crate::forge::{self, PullRequest};
use crate::interact::{self, Mode};
use crate::library::Library;
use crate::report::{Environment, Report, Signature};
use crate::ui;

/// What to do with foreign commits on the remote share branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Foreign {
    /// Bring them into the working branch.
    Merge,
    /// Discard them from the share branch: they are lost, unless kept elsewhere.
    Discard,
}

/// Options of `stemma share`.
pub struct Options {
    /// Share even when the pull request lacks what the person could give it.
    pub anyway: bool,
    /// Share up to this commit of the working branch, rather than HEAD.
    pub upto: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub yes: bool,
    pub foreign: Option<Foreign>,
    /// Build the library and run the checks before sharing.
    pub build: bool,
}

/// What sharing found and did, for the person and for agents.
#[derive(Default)]
struct Outcome {
    branch: String,
    share: String,
    /// Whether the work was on `main`, and moved to `branch`.
    moved_from_main: bool,
    /// Commits on the remote share branch that the working branch lacks.
    foreign: Vec<String>,
    /// Whether they were brought into the working branch.
    merged_foreign: bool,
    /// Whether the remote share branch was replaced (rewritten history, or
    /// foreign commits discarded).
    replaced: bool,
    /// Files in conflict, when bringing in `main` or foreign commits failed.
    conflicts: Vec<String>,
    checks: Option<Checks>,
    /// Central environments not signed yet (nothing waits for them).
    unsigned: Vec<String>,
    /// What the change lacks that the person can supply with `stemma sign`.
    needs_you: Vec<String>,
    /// What the change lacks that only someone else can supply.
    needs_others: Vec<String>,
    /// The commit shared.
    target: Option<String>,
    /// Signatures and approvals after the commit shared, so not included.
    left_out: Vec<String>,
    pushed: bool,
    pull_request: Option<String>,
    /// Whether an open pull request was updated rather than opened.
    updated: bool,
    /// Whether the open pull request comes from the working branch itself, as
    /// earlier versions of stemma opened them.
    from_working_branch: bool,
    /// Why the forge could not open the pull request.
    forge_error: Option<String>,
    /// Why sharing stopped before pushing, and what to do.
    stopped: Option<String>,
}

/// One line per commit, as `<short hash> <subject>`.
fn commits(dir: &Path, args: &[&str]) -> Vec<String> {
    let mut all = vec!["log", "--format=%h %s"];
    all.extend(args);
    git_ok(dir, &all)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The commits `branch` pointed at before: its reflog here, and where it is
/// saved on the remotes (with their reflogs).
fn earlier_tips(dir: &Path, branch: &str) -> Vec<String> {
    let mut refs = vec![format!("refs/heads/{branch}")];
    for remote in [branches::save_remote(dir), ORIGIN.to_string()] {
        refs.push(format!("refs/remotes/{remote}/{branch}"));
    }
    let mut tips = Vec::new();
    for r in refs {
        if let Some(c) = commit_of(dir, &r) {
            tips.push(c);
        }
        if let Ok(log) = git_ok(dir, &["log", "-g", "--format=%H", &r]) {
            tips.extend(log.lines().map(str::to_string));
        }
    }
    tips.sort();
    tips.dedup();
    tips
}

/// The commits of `rev` that are neither in `excluded` nor in their history.
fn commits_outside(dir: &Path, rev: &str, excluded: &[String]) -> Result<Vec<String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .args(["rev-list", "--stdin"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git")?;
    let input = format!("{rev}\n--not\n{}\n", excluded.join("\n"));
    child
        .stdin
        .take()
        .context("writing to git")?
        .write_all(input.as_bytes())?;
    let out = child.wait_with_output()?;
    ensure!(
        out.status.success(),
        "`git rev-list` failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// The files in conflict, after a merge that failed.
fn conflicted(dir: &Path) -> Vec<String> {
    git_ok(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Merges `rev` into the working branch. Returns the files in conflict, which
/// are left for the person or the agent to resolve.
fn merge(dir: &Path, rev: &str) -> Result<Vec<String>> {
    let out = git(dir, &["merge", "--quiet", "--no-edit", rev])?;
    if out.status.success() {
        return Ok(Vec::new());
    }
    let conflicts = conflicted(dir);
    if conflicts.is_empty() {
        bail!(
            "merging {rev} failed:\n{}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(conflicts)
}

/// What the person's session shows, while it is interactive.
struct Screen(bool);

impl Screen {
    fn info(&self, text: impl std::fmt::Display) -> Result<()> {
        if self.0 {
            cliclack::log::info(text)?;
        }
        Ok(())
    }
    fn warning(&self, text: impl std::fmt::Display) -> Result<()> {
        if self.0 {
            cliclack::log::warning(text)?;
        }
        Ok(())
    }
    fn success(&self, text: impl std::fmt::Display) -> Result<()> {
        if self.0 {
            cliclack::log::success(text)?;
        }
        Ok(())
    }
    fn note(&self, title: impl std::fmt::Display, text: impl std::fmt::Display) -> Result<()> {
        if self.0 {
            cliclack::note(title, text)?;
        }
        Ok(())
    }
}

/// The environments of the modules this branch changes.
///
/// This is where the environments that are new on the branch belong, once the
/// library can tell them apart (by their labels on `main`); until then, those
/// of the modules the branch touches are the closest it knows.
fn branch_environments<'a>(library: &Library, report: &'a Report) -> Vec<&'a Environment> {
    let changed = git_ok(
        &library.dir,
        &["diff", "--name-only", &format!("{ORIGIN}/{MAIN}...HEAD")],
    )
    .unwrap_or_default();
    let changed: Vec<&str> = changed.lines().collect();
    report
        .environments
        .iter()
        .filter(|env| {
            let path = library.module_path(&env.record.module);
            path.strip_prefix(&library.dir)
                .ok()
                .and_then(|p| p.to_str())
                .is_some_and(|p| changed.contains(&p))
        })
        .collect()
}

/// What the change lacks: what the person can supply, and what only others can.
fn missing(library: &Library, report: Option<&Report>) -> Result<(Vec<String>, Vec<String>)> {
    let dir = &library.dir;
    let verdict = crate::verify::evaluate(library, &format!("{ORIGIN}/{MAIN}"), report)?;
    let me = crate::keys::signing_key(dir).and_then(|key| {
        verdict
            .base_config
            .member_with_key(&key)
            .map(str::to_string)
    });
    let (mut mine, mut others) = (Vec::new(), Vec::new());
    for m in &verdict.missing {
        if me
            .as_deref()
            .is_some_and(|me| m.can_supply(&verdict.base_config, me))
        {
            mine.push(m.describe());
        } else {
            others.push(m.describe());
        }
    }
    Ok((mine, others))
}

/// The pull request's title and body, from the commits it carries.
fn default_text(dir: &Path, branch: &str, target: &str) -> (String, String) {
    let main = format!("{ORIGIN}/{MAIN}");
    let subjects: Vec<String> = git_ok(
        dir,
        &[
            "log",
            "--reverse",
            "--no-merges",
            "--format=%s",
            target,
            "--not",
            &main,
        ],
    )
    .map(|s| s.lines().map(str::to_string).collect())
    .unwrap_or_default();
    let title = match subjects.as_slice() {
        [one] => one.clone(),
        _ => format!("Work from {branch}"),
    };
    let mut body: String = subjects.iter().map(|s| format!("- {s}\n")).collect();
    body.push_str(&format!("\nShared with `stemma share` from `{branch}`."));
    (title, body)
}

/// What to do about the remote share branch, before anything else.
enum Remote {
    /// It is absent, or an ancestor of the working branch: it moves forward.
    Forward,
    /// It is replaced, guarded by the commit it was seen at.
    Replace(String),
}

/// Looks at the remote share branch `rs` against the working branch: moves
/// forward, replaces it after asking, or brings its foreign commits in. Sets
/// `o.stopped` (or `o.conflicts`) when sharing cannot go on.
fn reconcile(
    dir: &Path,
    o: &mut Outcome,
    rs: &str,
    options: &Options,
    mode: Mode,
    screen: &Screen,
) -> Result<Remote> {
    let (branch, share) = (o.branch.clone(), o.share.clone());
    let main = format!("{ORIGIN}/{MAIN}");
    if is_ancestor(dir, rs, "HEAD") {
        return Ok(Remote::Forward);
    }
    let mut known = vec!["HEAD".to_string(), main.clone()];
    known.extend(earlier_tips(dir, &branch));
    let foreign = commits_outside(dir, rs, &known)?;
    let lacking = commits(dir, &[rs, "--not", "HEAD", &main]);
    if foreign.is_empty() {
        // Every commit it has came from the earlier history of the branch.
        let question = format!(
            "{branch} was rewritten after it was shared: {share} has {} {} that {branch} no \
             longer has, all from its earlier history. Replace them with its new history",
            lacking.len(),
            if lacking.len() == 1 {
                "commit"
            } else {
                "commits"
            }
        );
        screen.note(
            format!("On {share}, and no longer on {branch}"),
            lacking.join("\n"),
        )?;
        let replace = if mode.interactive() {
            interact::confirm(format!("{question}?"), true)?
        } else if mode.agent_session() {
            o.stopped = Some(format!(
                "{question}? That forces a push, which is a person's decision: ask them to run \
                 `stemma share` in their own terminal."
            ));
            return Ok(Remote::Forward);
        } else if mode.yes() {
            true
        } else {
            o.stopped = Some(interact::needs_flag(question, "--yes").to_string());
            return Ok(Remote::Forward);
        };
        if !replace {
            o.stopped = Some(format!("Not shared: {share} keeps its commits."));
            return Ok(Remote::Forward);
        }
        o.replaced = true;
        return Ok(Remote::Replace(rs.to_string()));
    }
    o.foreign = lacking.clone();
    screen.note(
        format!("Commits on {share} that {branch} does not have"),
        lacking.join("\n"),
    )?;
    let decision = match options.foreign {
        Some(decision) => decision,
        None if mode.interactive() => {
            let items = vec![
                (
                    Foreign::Merge,
                    format!("Bring them into {branch}"),
                    "nothing is lost".to_string(),
                ),
                (
                    Foreign::Discard,
                    "Discard them".to_string(),
                    format!("{share} is replaced"),
                ),
            ];
            interact::select(
                format!(
                    "{share} has commits made elsewhere (the forge's \"Update branch\", an \
                     accepted suggestion, another push)."
                ),
                &items,
                Some(Foreign::Merge),
            )?
        }
        None => {
            o.stopped = Some(
                interact::needs_flag(
                    format!(
                        "{share} has commits made elsewhere, which {branch} does not have: \
                         {}. Nothing was pushed. Bring them in (`git merge {ORIGIN}/{share}`, \
                         or --foreign merge), or have a person discard them (--foreign \
                         discard, outside an agent's session)",
                        lacking.join("; ")
                    ),
                    "--foreign merge",
                )
                .to_string(),
            );
            return Ok(Remote::Forward);
        }
    };
    match decision {
        Foreign::Merge => {
            o.conflicts = merge(dir, &format!("refs/remotes/{ORIGIN}/{share}"))?;
            if o.conflicts.is_empty() {
                o.merged_foreign = true;
                screen.success(format!("Brought them into {branch}."))?;
            }
            Ok(Remote::Forward)
        }
        Foreign::Discard => {
            if mode.agent_session() {
                o.stopped = Some(format!(
                    "Discarding commits on {share} loses them, which is a person's decision: \
                     ask them to run `stemma share` in their own terminal."
                ));
                return Ok(Remote::Forward);
            }
            if mode.interactive() {
                let lost: Vec<String> = foreign
                    .iter()
                    .filter_map(|c| commits(dir, &["-1", c]).into_iter().next())
                    .collect();
                screen.note("These commits will be lost", lost.join("\n"))?;
                let discard = interact::confirm(
                    format!(
                        "Discard these {} commits? They are on no branch of yours, nor in main.",
                        lost.len()
                    ),
                    false,
                )?;
                if !discard {
                    o.stopped = Some(format!("Not shared: {share} keeps its commits."));
                    return Ok(Remote::Forward);
                }
            }
            o.replaced = true;
            Ok(Remote::Replace(rs.to_string()))
        }
    }
}

/// Shares the working branch.
fn share_branch(library: &Library, options: &Options, mode: Mode, json: bool) -> Result<Outcome> {
    let dir = &library.dir;
    let screen = Screen(mode.interactive());
    let Some(mut branch) = branches::current(dir) else {
        bail!("share from a branch: HEAD is on none");
    };
    ensure!(
        !branches::is_share(&branch),
        "{branch} is a share branch, which nobody works on: switch to its working branch, \
         work/{}, and share from there",
        branch.trim_start_matches("share/")
    );
    // Work done on `main` moves to a working branch of its own: nothing is
    // pushed to `main`, and nothing is lost.
    let mut o = Outcome::default();
    if branch == MAIN {
        branch =
            branches::move_to_new(dir, &branches::working_name(&crate::agent::person(), None))?;
        o.moved_from_main = true;
    }
    if branches::has_uncommitted_changes(dir) {
        if o.moved_from_main {
            bail!(
                "your work was on `main`, and is now on the branch {branch}; it has \
                 uncommitted changes: commit them, then share again"
            );
        }
        bail!("there are uncommitted changes: commit them before sharing");
    }
    let share = branches::share_of(&branch);
    o.branch = branch.clone();
    o.share = share.clone();
    if mode.interactive() {
        interact::intro("share")?;
    }
    git_ok(dir, &["fetch", "--quiet", "--prune", ORIGIN])
        .context("fetching the group's repository")?;
    let main = format!("{ORIGIN}/{MAIN}");
    ensure!(
        commit_of(dir, &main).is_some(),
        "the remote has no `main` yet: a person publishes the library first, with \
         `git push origin main`"
    );

    // The pull requests: from the share branch, or, as earlier versions
    // opened them, from the working branch itself.
    let forge = forge::current();
    let pull_requests = match forge.pull_requests(dir, &[&share, &branch]) {
        Ok(prs) => prs,
        Err(error) => {
            screen.warning(format!(
                "Could not ask the forge for pull requests: {error:#}"
            ))?;
            Vec::new()
        }
    };
    let open_from = |head: &str| {
        pull_requests
            .iter()
            .find(|pr| pr.state == forge::State::Open && pr.head == head)
            .cloned()
    };
    let open: Option<PullRequest> = open_from(&share).or_else(|| open_from(&branch));
    o.from_working_branch = open.as_ref().is_some_and(|pr| pr.head == branch);
    // The branch the pull request comes from, and where it is on the remote.
    let head = if o.from_working_branch {
        branch.clone()
    } else {
        share.clone()
    };
    let remote_head = commit_of(dir, &format!("refs/remotes/{ORIGIN}/{head}"));

    // 1. The remote share branch: forward, rewritten, or with foreign
    // commits. A pull request from the working branch itself is only ever
    // moved forward: git refuses anything else.
    let mut lease = None;
    if let Some(rs) = &remote_head
        && !o.from_working_branch
    {
        if let Remote::Replace(seen) = reconcile(dir, &mut o, rs, options, mode, &screen)? {
            lease = Some(seen);
        }
        if o.stopped.is_some() || !o.conflicts.is_empty() {
            return Ok(o);
        }
    }

    // 2. Bring `main` in.
    if !is_ancestor(dir, &main, "HEAD") {
        o.conflicts = merge(dir, &main)?;
        if !o.conflicts.is_empty() {
            return Ok(o);
        }
        screen.success(format!("Brought what is new on main into {branch}."))?;
    }

    // 3. The checks.
    let report = if options.build {
        let mut checks = commands::run_checks(library, json || mode.interactive())?;
        let ok = checks.ok();
        let report = checks.report.take();
        o.checks = Some(checks);
        if !ok {
            return Ok(o);
        }
        report
    } else {
        None
    };

    // 4. The state.
    let ahead = commits(dir, &["--first-parent", "HEAD", "--not", &main]);
    let mut state = vec![format!(
        "{branch}: {} {} ahead of main",
        ahead.len(),
        if ahead.len() == 1 {
            "commit"
        } else {
            "commits"
        }
    )];
    if let Some(report) = &report {
        for env in &report.environments {
            if env.signature(dir) == Some(Signature::Unsigned) {
                o.unsigned
                    .push(env.record.label.clone().unwrap_or_default());
            }
        }
        let envs = branch_environments(library, report);
        let claims: Vec<_> = envs
            .iter()
            .filter(|e| matches!(e.record.base.as_str(), "definition" | "statement"))
            .collect();
        let central = claims.iter().filter(|e| e.record.central).count();
        let signed = claims
            .iter()
            .filter(|e| e.signature(dir) == Some(Signature::Signed))
            .count();
        state.push(format!(
            "Environments in the modules it changes: {} ({central} central, {signed} signed)",
            envs.len()
        ));
        state.push("Checks: every check passes".into());
        if !claims.is_empty() && central == 0 {
            screen.warning(
                "This branch has definitions or statements, and none is central: nothing in \
                 it will be signed. Make the ones that matter central first.",
            )?;
        }
    } else {
        state.push("Checks: not run (--no-build); the pull request runs them".into());
    }
    state.push("Main: brought in, without conflicts".into());
    screen.note("What you are sharing", state.join("\n"))?;

    // 5. What is missing.
    let mut signed_now = false;
    loop {
        (o.needs_you, o.needs_others) = missing(library, report.as_ref())?;
        if o.needs_you.is_empty() || options.anyway {
            break;
        }
        if !mode.interactive() {
            o.stopped = Some(
                "Not shared yet: the pull request would lack what you can give it. Run `stemma \
                 sign` in your own terminal, then share again (--anyway shares now)."
                    .into(),
            );
            return Ok(o);
        }
        screen.note("What you can give it", o.needs_you.join("\n"))?;
        let items = vec![
            (0, "Sign now".to_string(), "runs `stemma sign`".to_string()),
            (
                1,
                "Share without it".to_string(),
                "its checks fail until it is given".to_string(),
            ),
            (2, "Stop".to_string(), String::new()),
        ];
        match interact::select("The pull request would lack these.", &items, Some(0))? {
            0 => {
                crate::sign::sign(Vec::new(), main.clone(), true)?;
                signed_now = true;
            }
            1 => break,
            _ => {
                o.stopped = Some("Not shared.".into());
                return Ok(o);
            }
        }
    }
    if !o.needs_others.is_empty() {
        screen.note("What only others can give", o.needs_others.join("\n"))?;
    }

    // 6. What to share: up to HEAD, or to an earlier commit. Signing makes it
    // HEAD, so that the signatures are shared.
    let shared_already = remote_head
        .clone()
        .filter(|rs| lease.is_none() && is_ancestor(dir, rs, "HEAD"));
    let head_commit = commit_of(dir, "HEAD").context("reading HEAD")?;
    let target = if let Some(upto) = &options.upto {
        let Some(t) = commit_of(dir, upto) else {
            bail!("--upto {upto}: no such commit");
        };
        ensure!(
            is_ancestor(dir, &t, "HEAD"),
            "--upto {upto}: not a commit of {branch}"
        );
        if let Some(rs) = &shared_already {
            ensure!(
                t == *rs || !is_ancestor(dir, &t, rs),
                "--upto {upto}: {head} already goes beyond it"
            );
        }
        t
    } else if mode.interactive() && !signed_now {
        let mut args = vec![
            "log",
            "--first-parent",
            "--format=%H%x09%h %s",
            "HEAD",
            "--not",
            &main,
        ];
        if let Some(rs) = &shared_already {
            args.push(rs);
        }
        let listed = git_ok(dir, &args)?;
        let items: Vec<(String, String, String)> = listed
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .enumerate()
            .map(|(i, (sha, line))| {
                let hint = if i == 0 { "everything" } else { "" };
                (sha.to_string(), line.to_string(), hint.to_string())
            })
            .collect();
        if items.len() > 1 {
            interact::select(
                "Share up to which commit?",
                &items,
                Some(head_commit.clone()),
            )?
        } else {
            head_commit.clone()
        }
    } else {
        head_commit.clone()
    };
    o.target = Some(target.clone());
    if target != head_commit {
        let range = format!("{target}..HEAD");
        let mut left_out = commits(dir, &[&range, "--", "signatures"]);
        for c in commits(dir, &["--extended-regexp", "--grep=^Approve:", &range]) {
            if !left_out.contains(&c) {
                left_out.push(c);
            }
        }
        o.left_out = left_out;
        let later = commits(dir, &[&range]).len();
        screen.warning(format!(
            "Sharing up to an earlier commit: the {later} later commits are not included{}",
            if o.left_out.is_empty() {
                ".".to_string()
            } else {
                format!(
                    ", nor the signatures and approvals among them:\n{}",
                    o.left_out.join("\n")
                )
            }
        ))?;
    }
    if is_ancestor(dir, &target, &main) {
        o.stopped = Some("Nothing to share: all of it is in main.".into());
        return Ok(o);
    }
    let up_to_date = remote_head.as_ref() == Some(&target) && lease.is_none();
    if up_to_date && open.is_some() {
        o.pull_request = open.map(|pr| pr.url);
        o.updated = true;
        screen.info("The pull request already has all of it.")?;
        return Ok(o);
    }

    // 7. An open pull request is updated.
    if let Some(pr) = &open
        && mode.interactive()
        && !interact::confirm(
            format!(
                "Update the open pull request #{} ({})? Approvals of its current content stop \
                 counting: an approval names the content it approves.",
                pr.number, pr.url
            ),
            true,
        )?
    {
        o.stopped = Some("Not shared: the pull request is as it was.".into());
        return Ok(o);
    }
    if o.from_working_branch {
        screen.info(format!(
            "This pull request comes from {branch} itself, as earlier versions of stemma \
             opened them: it is updated as before. Once it is merged, sharing goes through \
             {share}."
        ))?;
    }

    // 8. The title and body of a new pull request.
    let (default_title, default_body) = default_text(dir, &branch, &target);
    let mut title = options.title.clone().unwrap_or(default_title);
    let mut body = options.body.clone().unwrap_or(default_body);
    if open.is_none() && mode.interactive() {
        if options.title.is_none() {
            title = interact::text("The pull request's title", &title, |t| {
                t.trim().is_empty().then_some("a title is needed")
            })?;
        }
        if options.body.is_none() {
            body = interact::paragraph("Its description (Esc, then Enter, to go on)", &body)?;
        }
    }

    // 9. Confirm, push, and open the pull request.
    if mode.interactive() {
        let count = commits(dir, &[&target, "--not", &main]).len();
        let replacing = if o.replaced {
            format!(", replacing what {head} has on the remote")
        } else {
            String::new()
        };
        let question = format!(
            "Share {count} {} of {branch} as {head}{replacing}?",
            if count == 1 { "commit" } else { "commits" }
        );
        if !interact::confirm(question, true)? {
            o.stopped = Some("Not shared.".into());
            return Ok(o);
        }
    }
    // The invariant, once more: what is pushed is a commit of the working branch.
    ensure!(
        is_ancestor(dir, &target, &format!("refs/heads/{branch}")),
        "{target} is not a commit of {branch}: nothing was pushed"
    );
    if !up_to_date {
        let refspec = format!("{target}:refs/heads/{head}");
        let lease = lease.map(|rs| format!("--force-with-lease=refs/heads/{head}:{rs}"));
        let mut args = vec!["push", "--quiet"];
        args.extend(lease.as_deref());
        args.extend([ORIGIN, refspec.as_str()]);
        let out = git(dir, &args)?;
        if !out.status.success() {
            bail!(
                "pushing {head} failed, and nothing was overwritten; someone may have pushed \
                 to it meanwhile: share again to see what changed.\n{}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        o.pushed = true;
    }
    match open {
        Some(pr) => {
            o.pull_request = Some(pr.url);
            o.updated = true;
        }
        None => {
            let new = forge::NewPullRequest {
                head: &head,
                base: MAIN,
                title: &title,
                body: &body,
            };
            match forge.open_pull_request(dir, &new) {
                Ok(pr) => o.pull_request = Some(pr.url),
                Err(error) => o.forge_error = Some(format!("{error:#}")),
            }
        }
    }
    Ok(o)
}

/// `stemma share`.
pub fn share(options: Options, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let mode = Mode::detect(json_output, options.yes);
    let o = share_branch(&library, &options, mode, json_output)?;
    let checks_ok = o.checks.as_ref().is_none_or(Checks::ok);
    let shared = o.pull_request.is_some() || o.pushed;
    let ok = o.conflicts.is_empty() && checks_ok && o.stopped.is_none() && shared;
    if json_output {
        let (problems, build) = o
            .checks
            .as_ref()
            .map_or((vec![], None), |c| (c.problems.clone(), c.build.clone()));
        let value = json!({
            "ok": ok, "branch": o.branch, "share_branch": o.share,
            "moved_from_main": o.moved_from_main,
            "foreign": o.foreign, "merged_foreign": o.merged_foreign, "replaced": o.replaced,
            "conflicts": o.conflicts,
            "problems": problems, "build": build,
            "target": o.target, "left_out": o.left_out,
            "pushed": o.pushed, "pull_request": o.pull_request, "updated": o.updated,
            "from_working_branch": o.from_working_branch, "forge_error": o.forge_error,
            "needs_you": o.needs_you, "needs_others": o.needs_others,
            "unsigned": o.unsigned, "stopped": o.stopped,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(ok);
    }
    let interactive = mode.interactive();
    if o.moved_from_main {
        ui::note(format!(
            "Your work was on `main`: it is now on the branch {}.",
            ui::bold(&o.branch)
        ));
    }
    if !o.conflicts.is_empty() {
        let text = format!(
            "Bringing changes into {} left conflicts in:\n{}\nResolve them, commit, and share \
             again.",
            o.branch,
            o.conflicts.join("\n")
        );
        if interactive {
            interact::outro_cancel(text)?;
        } else {
            ui::error(text);
        }
        return Ok(false);
    }
    if let Some(checks) = &o.checks
        && !checks.ok()
    {
        if interactive {
            interact::outro_cancel("Not shared: a check fails.")?;
        }
        if let Some(build) = &checks.build {
            commands::show_build_errors(build);
        }
        for p in &checks.problems {
            ui::error(p);
        }
        return Ok(false);
    }
    if let Some(stopped) = &o.stopped {
        if interactive {
            interact::outro_cancel(stopped)?;
        } else {
            for n in &o.needs_you {
                ui::error(n);
            }
            ui::warning(stopped);
        }
        return Ok(false);
    }
    let link = match (&o.pull_request, &o.forge_error) {
        (Some(url), _) if o.updated => format!(
            "Shared {} as {}: the pull request is updated, {url}",
            ui::bold(&o.branch),
            if o.from_working_branch {
                &o.branch
            } else {
                &o.share
            }
        ),
        (Some(url), _) => format!("Shared {} as {}: {url}", ui::bold(&o.branch), o.share),
        (None, error) => format!(
            "Pushed {}. Open a pull request from it to main: the forge could not ({}).",
            o.share,
            error.as_deref().unwrap_or("unknown")
        ),
    };
    let follow: Vec<&String> = o.needs_you.iter().chain(&o.needs_others).collect();
    if interactive {
        for n in &follow {
            cliclack::log::warning(n)?;
        }
        interact::outro(link)?;
    } else {
        if o.pull_request.is_some() {
            ui::success(link);
        } else {
            ui::warning(link);
        }
        for n in &follow {
            ui::warning(n);
        }
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
