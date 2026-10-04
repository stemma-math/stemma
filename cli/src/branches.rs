//! The branches `stemma` works on: nothing is done directly on `main`.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, ensure};

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
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(dir)
        .output()
        .is_ok_and(|o| o.status.success())
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

/// Switches to `branch`, creating it where HEAD is when it does not exist.
pub fn switch_to(dir: &Path, branch: &str) -> Result<()> {
    let mut switch = Command::new("git");
    switch.args(["switch", "--quiet"]).current_dir(dir);
    if !exists(dir, branch) {
        switch.arg("--create");
    }
    ensure!(
        switch.arg(branch).status()?.success(),
        "could not switch to the branch {branch}"
    );
    Ok(())
}
