//! The branches `stemma` works on: nothing is done directly on `main`.
//!
//! - **Working branches** are `work/<person>` and `work/<person>-<topic>`. They
//!   are never `work/<person>/<topic>`: git cannot hold `work/<person>` and
//!   `work/<person>/…` at once.
//! - **Share branches**, `share/<x>` for `work/<x>`, carry work to the group:
//!   they only ever point at a commit of their working branch, and nobody
//!   works on them (see `share.rs`).
//! - Working branches are saved to the person's **personal remote** when the
//!   clone has one (in its local git configuration), and to the group's
//!   repository, `origin`, otherwise. Share branches always go to `origin`.

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail, ensure};

use crate::forge::{self, Forge};
use crate::interact::{self, Mode};
use crate::ui;

/// The shared branch.
pub const MAIN: &str = "main";

/// The group's repository.
pub const ORIGIN: &str = "origin";

/// The local git setting that names the personal remote.
pub const PERSONAL_REMOTE_KEY: &str = "stemma.personalRemote";

/// The name `stemma remote` gives the personal remote.
pub const PERSONAL_REMOTE_NAME: &str = "personal";

/// The local git setting that records that the person was offered a personal
/// remote, so that they are asked once.
const PERSONAL_REMOTE_ASKED_KEY: &str = "stemma.personalRemoteAsked";

/// Runs git in `dir`.
pub fn git(dir: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running `git {}`", args.join(" ")))
}

/// Runs git in `dir`, failing with its error, and returns its output.
pub fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git(dir, args)?;
    if !out.status.success() {
        bail!(
            "`git {}` failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether a git command succeeds.
pub fn git_succeeds(dir: &Path, args: &[&str]) -> bool {
    git(dir, args).is_ok_and(|o| o.status.success())
}

/// The commit a revision names, if it names one.
pub fn commit_of(dir: &Path, rev: &str) -> Option<String> {
    git_ok(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// Whether `ancestor` is `descendant` or one of its ancestors.
pub fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> bool {
    git_succeeds(dir, &["merge-base", "--is-ancestor", ancestor, descendant])
}

/// The current branch, if the library is a git repository and HEAD is on one.
pub fn current(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()?;
    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !branch.is_empty()).then_some(branch)
}

/// Whether a local branch exists.
pub fn exists(dir: &Path, branch: &str) -> bool {
    commit_of(dir, &format!("refs/heads/{branch}")).is_some()
}

/// Whether the working tree has uncommitted changes.
pub fn has_uncommitted_changes(dir: &Path) -> bool {
    git_ok(dir, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty())
}

/// `base`, or `base-2`, `base-3`, … : the first name no branch has.
pub fn unused_name(dir: &Path, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while exists(dir, &name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// Moves the work at hand to a new branch named after `base`: the branch starts
/// where HEAD is, and takes the uncommitted changes with it, so nothing is lost.
pub fn move_to_new(dir: &Path, base: &str) -> Result<String> {
    let name = unused_name(dir, base);
    let status = Command::new("git")
        .args(["switch", "--quiet", "--create", &name])
        .current_dir(dir)
        .status()?;
    ensure!(status.success(), "could not create the branch {name}");
    Ok(name)
}

// Names.

/// Whether `branch` is a share branch, which nobody works on.
pub fn is_share(branch: &str) -> bool {
    branch.starts_with("share/")
}

/// The share branch of a working branch: `share/<x>` for `work/<x>`, and
/// `share/<branch>` for a branch of another form.
pub fn share_of(branch: &str) -> String {
    format!("share/{}", branch.strip_prefix("work/").unwrap_or(branch))
}

/// The working branch of `person`, with a topic or without.
pub fn working_name(person: &str, topic: Option<&str>) -> String {
    match topic {
        Some(topic) => format!("work/{person}-{topic}"),
        None => format!("work/{person}"),
    }
}

/// What is wrong with a topic, if anything.
pub fn topic_problem(topic: &str) -> Option<&'static str> {
    if topic.is_empty() {
        Some("a topic is needed")
    } else if !topic
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        Some("lowercase letters, digits and -")
    } else if topic.starts_with('-') || topic.ends_with('-') || topic.contains("--") {
        Some("a topic does not start or end with -, nor repeat it")
    } else {
        None
    }
}

/// Checks the name of a new working branch: `work/<person>` or
/// `work/<person>-<topic>`, never `work/<person>/<topic>`.
pub fn check_new_working_name(dir: &Path, name: &str) -> Result<()> {
    let Some(rest) = name.strip_prefix("work/") else {
        bail!("a new working branch is named work/<person> or work/<person>-<topic>, not {name}");
    };
    if rest.contains('/') {
        bail!(
            "{name}: a working branch is work/<person> or work/<person>-<topic>, never \
             work/<person>/<topic> (git cannot hold work/<person> beside it)"
        );
    }
    ensure!(
        !rest.is_empty() && git_succeeds(dir, &["check-ref-format", "--branch", name]),
        "{name} is not a valid branch name"
    );
    Ok(())
}

// Where working branches are saved.

/// The personal remote, when the clone has one.
pub fn personal_remote(dir: &Path) -> Option<String> {
    let name = git_ok(dir, &["config", "--local", "--get", PERSONAL_REMOTE_KEY]).ok()?;
    let exists = git_succeeds(dir, &["remote", "get-url", &name]);
    (!name.is_empty() && exists).then_some(name)
}

/// Where working branches are saved: the personal remote, or `origin`.
pub fn save_remote(dir: &Path) -> String {
    personal_remote(dir).unwrap_or_else(|| ORIGIN.to_string())
}

/// Makes `git push` on a working branch save it to `remote`, under its own
/// name: the branch's push remote, and an upstream set on its first push.
fn save_to(dir: &Path, branch: &str, remote: &str) -> Result<()> {
    git_ok(
        dir,
        &[
            "config",
            "--local",
            &format!("branch.{branch}.pushRemote"),
            remote,
        ],
    )?;
    git_ok(dir, &["config", "--local", "push.autoSetupRemote", "true"])?;
    Ok(())
}

/// The local working branches.
fn local_working_branches(dir: &Path) -> Vec<String> {
    git_ok(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=2)",
            "refs/heads/work/",
        ],
    )
    .map(|s| s.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

/// Sets the personal remote to `url`: working branches are saved there from
/// now on, and only share branches go to the group's repository.
pub fn set_personal_remote(dir: &Path, url: &str) -> Result<()> {
    let name = PERSONAL_REMOTE_NAME;
    if git_succeeds(dir, &["remote", "get-url", name]) {
        git_ok(dir, &["remote", "set-url", name, url])?;
    } else {
        git_ok(dir, &["remote", "add", name, url])?;
    }
    git_ok(dir, &["config", "--local", PERSONAL_REMOTE_KEY, name])?;
    git_ok(
        dir,
        &["config", "--local", PERSONAL_REMOTE_ASKED_KEY, "true"],
    )?;
    for branch in local_working_branches(dir) {
        save_to(dir, &branch, name)?;
    }
    Ok(())
}

/// Stops using a personal remote: working branches are saved to the group's
/// repository again. The remote itself stays, with everything on it.
pub fn unset_personal_remote(dir: &Path) -> Result<()> {
    let _ = git(dir, &["config", "--local", "--unset", PERSONAL_REMOTE_KEY]);
    for branch in local_working_branches(dir) {
        save_to(dir, &branch, ORIGIN)?;
    }
    Ok(())
}

/// `stemma remote`: shows, sets or unsets the personal remote.
pub fn remote_command(url: Option<String>, unset: bool, json_output: bool) -> Result<bool> {
    let library = crate::library::Library::find(Path::new("."))?;
    let dir = &library.dir;
    ensure!(
        !(unset && url.is_some()),
        "a URL and --unset say different things: pass one"
    );
    if unset {
        unset_personal_remote(dir)?;
    } else if let Some(url) = &url {
        set_personal_remote(dir, url)?;
    }
    let personal = personal_remote(dir);
    let url = personal
        .as_deref()
        .and_then(|r| git_ok(dir, &["remote", "get-url", r]).ok());
    if json_output {
        let value = serde_json::json!({
            "personal_remote": personal, "url": url, "save_remote": save_remote(dir),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(true);
    }
    match (&personal, &url) {
        (Some(name), Some(url)) => {
            ui::success(format!(
                "Working branches are saved to your personal remote, {} ({url}).",
                ui::bold(name)
            ));
            ui::note("Only share branches (share/…) go to the group's repository.");
        }
        _ => ui::success(
            "Working branches are saved to the group's repository (origin): there is no \
             personal remote.",
        ),
    }
    Ok(true)
}

// The branch an agent works on.

/// What the person asked for, with flags, when starting an agent.
#[derive(Default)]
pub struct Request {
    /// `--branch`: an existing branch, or a new working branch from `main`.
    pub branch: Option<String>,
    /// `--here`: the current branch.
    pub here: bool,
    /// `--delete-merged`: delete the person's branches whose content is in
    /// `main`.
    pub delete_merged: bool,
}

/// A working branch, and its state.
#[derive(Clone, Debug)]
struct Info {
    name: String,
    /// Whether it exists only on the remote it is saved to.
    remote_only: bool,
    /// Commits not in `main`.
    ahead: usize,
    /// The state of its latest pull request, from its share branch or itself.
    pull_request: Option<forge::State>,
    /// Whether it is the current branch and has uncommitted changes.
    dirty: bool,
    /// Whether all its content, here and where it is saved, is in `main`.
    in_main: bool,
}

impl Info {
    fn merged(&self) -> bool {
        self.in_main && self.pull_request == Some(forge::State::Merged)
    }

    /// Whether the picker offers to delete it.
    fn deletable(&self) -> bool {
        self.in_main && !self.dirty
    }

    fn describe(&self, current: bool) -> String {
        let mut parts = Vec::new();
        if current {
            parts.push("current".to_string());
        }
        if self.remote_only {
            parts.push("saved on its remote only".into());
        }
        if self.merged() {
            parts.push("merged".into());
        } else if self.in_main {
            parts.push("nothing new: all in main".into());
        } else {
            parts.push(format!(
                "{} {} ahead of main",
                self.ahead,
                if self.ahead == 1 { "commit" } else { "commits" }
            ));
            match self.pull_request {
                Some(forge::State::Open) => parts.push("pull request open".into()),
                Some(forge::State::Merged) => parts.push("a pull request merged".into()),
                _ => {}
            }
        }
        if self.dirty {
            parts.push("uncommitted changes".into());
        }
        parts.join(" · ")
    }
}

/// The reference `main` is compared with: `origin/main` when the library has
/// a remote, `main` otherwise.
pub fn main_ref(dir: &Path) -> Option<String> {
    [format!("{ORIGIN}/{MAIN}"), MAIN.to_string()]
        .into_iter()
        .find(|r| commit_of(dir, r).is_some())
}

/// Fetches a remote, quietly. It fails only when the remote exists and cannot
/// be reached.
fn fetch(dir: &Path, remote: &str) -> Result<()> {
    if !git_succeeds(dir, &["remote", "get-url", remote]) {
        return Ok(());
    }
    git_ok(dir, &["fetch", "--quiet", "--prune", remote]).map(drop)
}

/// The commit a branch is saved at on `remote`, if it is.
fn saved_commit(dir: &Path, remote: &str, branch: &str) -> Option<String> {
    commit_of(dir, &format!("refs/remotes/{remote}/{branch}"))
}

/// Whether a branch's content, here and where it is saved, is all in `main`.
fn content_in_main(dir: &Path, branch: &str, main: &str) -> bool {
    let local = commit_of(dir, &format!("refs/heads/{branch}"));
    let saved = saved_commit(dir, &save_remote(dir), branch);
    if local.is_none() && saved.is_none() {
        return false;
    }
    local
        .iter()
        .chain(saved.iter())
        .all(|c| is_ancestor(dir, c, main))
}

/// The person's working branches (and the current branch, when it is a
/// working branch), with their states.
fn working_branches(
    dir: &Path,
    person: &str,
    forge: &dyn Forge,
    main: Option<&str>,
    current: &str,
) -> Vec<Info> {
    let own = working_name(person, None);
    let mine = |name: &str| {
        name == own
            || name
                .strip_prefix(&own)
                .is_some_and(|rest| rest.starts_with('-'))
    };
    let save = save_remote(dir);
    let mut names: Vec<(String, bool)> = local_working_branches(dir)
        .into_iter()
        .filter(|n| mine(n) || n == current)
        .map(|n| (n, false))
        .collect();
    let saved = git_ok(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=3)",
            &format!("refs/remotes/{save}/work/"),
        ],
    )
    .unwrap_or_default();
    for name in saved.lines() {
        if mine(name) && !names.iter().any(|(n, _)| n == name) {
            names.push((name.to_string(), true));
        }
    }
    let heads: Vec<String> = names
        .iter()
        .flat_map(|(n, _)| [share_of(n), n.clone()])
        .collect();
    let head_refs: Vec<&str> = heads.iter().map(String::as_str).collect();
    let pull_requests = if names.is_empty() {
        Vec::new()
    } else {
        forge.pull_requests(dir, &head_refs).unwrap_or_default()
    };
    let dirty = has_uncommitted_changes(dir);
    names
        .into_iter()
        .map(|(name, remote_only)| {
            let tip = if remote_only {
                format!("refs/remotes/{save}/{name}")
            } else {
                format!("refs/heads/{name}")
            };
            let ahead = main
                .and_then(|m| git_ok(dir, &["rev-list", "--count", &tip, "--not", m]).ok())
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let share = share_of(&name);
            let pull_request = pull_requests
                .iter()
                .find(|pr| pr.head == share || pr.head == name)
                .map(|pr| pr.state);
            Info {
                dirty: dirty && name == current,
                in_main: main.is_some_and(|m| content_in_main(dir, &name, m)),
                name,
                remote_only,
                ahead,
                pull_request,
            }
        })
        .collect()
}

/// Deletes branches whose content is in `main`, here and on their remotes
/// (with their share branches), checking again just before. It never deletes
/// `keep` (the branch the agent works on) nor the current branch, so never one
/// with uncommitted changes, nor a branch with an open pull request, and every
/// deletion on a remote is guarded by the commit it was seen at. Returns the
/// branches it deleted.
fn delete_in_main(
    dir: &Path,
    names: &[String],
    keep: &str,
    main: &str,
    forge: &dyn Forge,
) -> Vec<String> {
    let current = current(dir);
    let save = save_remote(dir);
    let mut deleted = Vec::new();
    for name in names {
        if name == keep || Some(name) == current.as_ref() || !content_in_main(dir, name, main) {
            continue;
        }
        let share = share_of(name);
        let open = forge
            .pull_requests(dir, &[&share, name])
            .map(|prs| prs.iter().any(|pr| pr.state == forge::State::Open));
        if !matches!(open, Ok(false)) {
            // An open pull request, or no way to know: keep it.
            continue;
        }
        let mut done = false;
        if let Some(local) = commit_of(dir, &format!("refs/heads/{name}"))
            && is_ancestor(dir, &local, main)
            && git_succeeds(
                dir,
                &["update-ref", "-d", &format!("refs/heads/{name}"), &local],
            )
        {
            let _ = git(
                dir,
                &[
                    "config",
                    "--local",
                    "--remove-section",
                    &format!("branch.{name}"),
                ],
            );
            done = true;
        }
        for (remote, branch) in [(save.as_str(), name.as_str()), (ORIGIN, share.as_str())] {
            if let Some(seen) = saved_commit(dir, remote, branch)
                && is_ancestor(dir, &seen, main)
                && git_succeeds(
                    dir,
                    &[
                        "push",
                        "--quiet",
                        &format!("--force-with-lease=refs/heads/{branch}:{seen}"),
                        remote,
                        &format!(":refs/heads/{branch}"),
                    ],
                )
            {
                done = true;
            }
        }
        if done {
            deleted.push(name.clone());
        }
    }
    deleted
}

/// Where a new branch starts: an up-to-date `main`. When HEAD is on `main`
/// with commits `origin/main` does not have, they are work, and the new
/// branch takes them along.
fn start_point(dir: &Path, main: Option<&str>) -> String {
    if current(dir).as_deref() == Some(MAIN)
        && let Some(m) = main
        && !is_ancestor(dir, "HEAD", m)
    {
        return "HEAD".into();
    }
    main.unwrap_or("HEAD").to_string()
}

/// The commit HEAD moves to when switching to `name`.
fn destination(dir: &Path, name: &str, main: Option<&str>) -> Option<String> {
    let save = save_remote(dir);
    if exists(dir, name) {
        commit_of(dir, &format!("refs/heads/{name}"))
    } else if let Some(saved) = saved_commit(dir, &save, name) {
        Some(saved)
    } else {
        commit_of(dir, &start_point(dir, main))
    }
}

/// Switches to `name`, creating it when it is new: from where it is saved,
/// or from `main`. With uncommitted changes, git carries them, or refuses
/// when they would be overwritten: nothing is lost.
fn switch(dir: &Path, name: &str, main: Option<&str>) -> Result<()> {
    let save = save_remote(dir);
    let out = if exists(dir, name) {
        git(dir, &["switch", "--quiet", name])?
    } else if saved_commit(dir, &save, name).is_some() {
        git(
            dir,
            &[
                "switch",
                "--quiet",
                "--create",
                name,
                "--track",
                &format!("{save}/{name}"),
            ],
        )?
    } else {
        let start = start_point(dir, main);
        let out = git(
            dir,
            &["switch", "--quiet", "--no-track", "--create", name, &start],
        )?;
        if out.status.success() {
            save_to(dir, name, &save)?;
        }
        out
    };
    if !out.status.success() {
        bail!(
            "could not switch to {name}:\n{}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Offers a personal remote, once, when a working branch is created.
fn offer_personal_remote(dir: &Path) -> Result<()> {
    let asked = git_ok(
        dir,
        &["config", "--local", "--get", PERSONAL_REMOTE_ASKED_KEY],
    )
    .is_ok_and(|v| v == "true");
    if asked || personal_remote(dir).is_some() {
        return Ok(());
    }
    git_ok(
        dir,
        &["config", "--local", PERSONAL_REMOTE_ASKED_KEY, "true"],
    )?;
    let wanted = interact::confirm(
        "Save your working branches to a personal remote? Work in progress is then backed \
         up without being visible to the group",
        false,
    )?;
    if !wanted {
        cliclack::log::remark(
            "Working branches are saved to the group's repository. `stemma remote <url>` \
             sets a personal remote later.",
        )?;
        return Ok(());
    }
    let url = interact::text("Its URL, such as git@github.com:you/library.git", "", |u| {
        u.trim().is_empty().then_some("a URL is needed")
    })?;
    set_personal_remote(dir, url.trim())?;
    cliclack::log::success(format!(
        "Working branches are saved to {} from now on.",
        url.trim()
    ))?;
    Ok(())
}

/// Says something to the person: in the picker's style when it is open.
fn say(interactive: bool, warning: bool, text: &str) -> Result<()> {
    match (interactive, warning) {
        (true, true) => cliclack::log::warning(text)?,
        (true, false) => cliclack::log::info(text)?,
        (false, true) => ui::warning(text),
        (false, false) => ui::note(text),
    }
    Ok(())
}

/// The branch an agent works on, from the flags, or chosen by the person in a
/// picker. Returns it, or `None` outside git.
///
/// - `--branch <name>`: an existing branch, or a new working branch from `main`.
/// - `--here`: the current branch.
/// - Otherwise, interactively, the picker; without interaction, as before:
///   from `main`, the person's working branch, `work/<person>`.
///
/// It never offers `main` nor a share branch, never discards anything, and
/// deletes only branches whose content is in `main`.
pub fn choose(dir: &Path, request: &Request, mode: Mode) -> Result<Option<String>> {
    let Some(here) = current(dir) else {
        if !git_succeeds(dir, &["rev-parse", "--git-dir"]) {
            return Ok(None);
        }
        bail!("HEAD is on no branch: switch to one first (`git switch <branch>`)");
    };
    if let Some(branch) = &request.branch {
        ensure!(
            !is_share(branch),
            "{branch} is a share branch, which nobody works on: work on its working branch, \
             work/{}",
            branch.trim_start_matches("share/")
        );
        ensure!(
            branch != MAIN,
            "nobody works on main directly: pass a working branch"
        );
        ensure!(
            !request.here,
            "--here and --branch say different things: pass one"
        );
    }
    if request.here {
        ensure!(
            here != MAIN && !is_share(&here),
            "--here: the current branch is {here}, which nobody works on: pass --branch <name>"
        );
    }
    let forge = forge::current();
    let person = crate::agent::person();
    let interactive = mode.interactive() && request.branch.is_none() && !request.here;
    if interactive {
        interact::intro("start")?;
    }
    // An up-to-date main, and the remotes' branches; offline, what is known.
    for remote in [ORIGIN.to_string(), save_remote(dir)] {
        if let Err(error) = fetch(dir, &remote) {
            say(
                interactive,
                true,
                &format!("Could not reach {remote}: using what is known here. ({error:#})"),
            )?;
        }
    }
    let main = main_ref(dir);
    let branches = working_branches(dir, &person, forge.as_ref(), main.as_deref(), &here);

    let mut deleting = Vec::new();
    let target = if let Some(branch) = &request.branch {
        if !exists(dir, branch) && saved_commit(dir, &save_remote(dir), branch).is_none() {
            check_new_working_name(dir, branch)?;
        }
        branch.clone()
    } else if request.here {
        here.clone()
    } else if interactive {
        let (target, chosen) = pick(dir, &here, &person, &branches, main.is_some())?;
        deleting = chosen;
        target
    } else if here == MAIN {
        working_name(&person, None)
    } else if is_share(&here) {
        return Err(interact::needs_flag(
            format!("{here} is a share branch, which nobody works on: which branch to work on"),
            "--branch <name>",
        ));
    } else {
        here.clone()
    };
    if request.delete_merged {
        deleting = branches
            .iter()
            .filter(|b| b.deletable())
            .map(|b| b.name.clone())
            .collect();
    }

    let mut working = here.clone();
    if target != here {
        let dirty = has_uncommitted_changes(dir);
        let moves = destination(dir, &target, main.as_deref()) != commit_of(dir, "HEAD");
        let carry = if !dirty || !moves {
            true
        } else if interactive {
            let items = vec![
                (
                    true,
                    format!("Carry them to {target}"),
                    "git refuses if that would overwrite anything".to_string(),
                ),
                (false, format!("Stay on {here}"), String::new()),
            ];
            interact::select(
                format!("There are uncommitted changes on {here}."),
                &items,
                Some(true),
            )?
        } else {
            say(
                false,
                true,
                &format!(
                    "There are uncommitted changes on {here}: staying on it rather than \
                     carrying them to {target}. Commit them first, or pass --here."
                ),
            )?;
            false
        };
        if carry {
            let new =
                !exists(dir, &target) && saved_commit(dir, &save_remote(dir), &target).is_none();
            match switch(dir, &target, main.as_deref()) {
                Ok(()) => {
                    working = target.clone();
                    if new {
                        say(interactive, false, &format!("Created {target}."))?;
                        if interactive {
                            offer_personal_remote(dir)?;
                        }
                    }
                }
                Err(error) if dirty => say(
                    interactive,
                    true,
                    &format!(
                        "{error:#}\nStaying on {here}, with its uncommitted changes: nothing \
                         was lost."
                    ),
                )?,
                Err(error) => return Err(error),
            }
        }
    }
    if !deleting.is_empty()
        && let Some(main) = &main
    {
        let deleted = delete_in_main(dir, &deleting, &working, main, forge.as_ref());
        if !deleted.is_empty() {
            say(
                interactive,
                false,
                &format!(
                    "Deleted {}, here and on the remote: their content is in main.",
                    deleted.join(", ")
                ),
            )?;
        }
    }
    if interactive {
        interact::outro(format!("Working on {}.", ui::bold(&working)))?;
    }
    Ok(Some(working))
}

/// The picker: which branch to work on, and which branches whose content is in
/// `main` to delete.
fn pick(
    dir: &Path,
    here: &str,
    person: &str,
    branches: &[Info],
    has_main: bool,
) -> Result<(String, Vec<String>)> {
    let mut deleting = Vec::new();
    let deletable: Vec<&Info> = branches.iter().filter(|b| b.deletable()).collect();
    if !deletable.is_empty() && has_main {
        let items: Vec<(String, String, String)> = deletable
            .iter()
            .map(|b| (b.name.clone(), b.name.clone(), b.describe(b.name == here)))
            .collect();
        deleting = interact::multiselect(
            "These branches have all their content in main. Delete any, here and on their \
             remote? (space selects, enter goes on)",
            &items,
        )?;
    }
    /// What the person can choose.
    #[derive(Clone, PartialEq, Eq)]
    enum Choice {
        Existing(String),
        NewWithTopic,
    }
    let mut items: Vec<(Choice, String, String)> = Vec::new();
    let ordered = branches
        .iter()
        .filter(|b| b.name == here)
        .chain(branches.iter().filter(|b| b.name != here));
    for b in ordered {
        if deleting.contains(&b.name) || b.name == MAIN || is_share(&b.name) {
            continue;
        }
        items.push((
            Choice::Existing(b.name.clone()),
            b.name.clone(),
            b.describe(b.name == here),
        ));
    }
    let own = working_name(person, None);
    if !branches.iter().any(|b| b.name.starts_with(&own)) && !exists(dir, &own) {
        items.push((
            Choice::Existing(own.clone()),
            own.clone(),
            "new, from main".into(),
        ));
    }
    items.push((
        Choice::NewWithTopic,
        "New branch…".into(),
        format!("work/{person}-<topic>, from main"),
    ));
    let initial = items.first().map(|(c, _, _)| c.clone());
    let choice = interact::select("Which branch should the agent work on?", &items, initial)?;
    let target = match choice {
        Choice::Existing(name) => name,
        Choice::NewWithTopic => {
            let prefix = format!("work/{person}-");
            let dir = dir.to_path_buf();
            let topic = interact::text(format!("Its topic: {prefix}<topic>"), "", move |t| {
                topic_problem(t).or_else(|| {
                    exists(&dir, &format!("{prefix}{t}")).then_some("that branch exists")
                })
            })?;
            working_name(person, Some(&topic))
        }
    };
    Ok((target, deleting))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_branches_mirror_working_branches() {
        assert_eq!(share_of("work/alice"), "share/alice");
        assert_eq!(share_of("work/alice-groups"), "share/alice-groups");
        assert_eq!(
            share_of("upgrade/stemma-0.4.0"),
            "share/upgrade/stemma-0.4.0"
        );
        assert!(is_share("share/alice") && !is_share("work/alice"));
    }

    #[test]
    fn topics_are_simple() {
        assert_eq!(topic_problem("groups"), None);
        assert_eq!(topic_problem("sylow-2"), None);
        assert!(topic_problem("").is_some());
        assert!(topic_problem("a/b").is_some());
        assert!(topic_problem("Groups").is_some());
        assert!(topic_problem("-x").is_some());
    }
}
