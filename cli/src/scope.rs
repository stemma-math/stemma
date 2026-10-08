//! What a branch brings: the environments it touches, and who last touched the
//! others.
//!
//! An environment belongs to the branch when the lines it spans intersect what
//! the branch changes against its base, or when its module is new. Everything
//! comes from git and the report: no text is parsed, and the base is not
//! built, so the answer is the same on every machine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::library::Library;
use crate::report::Environment;

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running `git {}`", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "`git {}` failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What a branch changes against its base, by file: the lines of the current
/// version it adds or rewrites, and the places where it removes lines.
#[derive(Debug, Default)]
pub struct Scope {
    /// Files that are new on the branch.
    new: BTreeSet<String>,
    /// For each changed file, its hunks as `(start, count)` on the current
    /// side: `count` lines from `start`, or, when `count` is 0, a removal
    /// right after line `start`.
    hunks: BTreeMap<String, Vec<(u32, u32)>>,
}

impl Scope {
    /// What the branch changes against `base`, from where it forked from it
    /// (`git diff <base>...`), up to the working tree, since the report the
    /// environments come from is built from it. Renames count as a removal
    /// and an addition, whatever git is configured to do, so that a module
    /// moved to another file belongs to the branch that moved it.
    pub fn of_branch(dir: &Path, base: &str) -> Result<Self> {
        let fork = git(dir, &["merge-base", base, "HEAD"])
            .with_context(|| format!("finding where this branch forked from {base}"))?;
        let fork = fork.trim();
        let diff = git(
            dir,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-renames",
                "--relative",
                "--unified=0",
                fork,
            ],
        )?;
        let mut scope = Self::parse(&diff);
        // Files git does not track yet are new, wholly.
        let untracked = git(dir, &["ls-files", "--others", "--exclude-standard"])?;
        scope.new.extend(untracked.lines().map(str::to_string));
        Ok(scope)
    }

    /// Reads a diff made with `--unified=0`.
    fn parse(diff: &str) -> Self {
        let mut scope = Self::default();
        let mut file: Option<String> = None;
        let mut from_nothing = false;
        // Whether we are in a file's header, before its first hunk: only there
        // do `---` and `+++` name files, not lines.
        let mut in_header = false;
        for line in diff.lines() {
            if line.starts_with("diff --git ") {
                file = None;
                from_nothing = false;
                in_header = true;
            } else if let Some(header) = line.strip_prefix("@@ ") {
                in_header = false;
                if let Some(f) = &file
                    && let Some(hunk) = parse_hunk(header)
                {
                    scope.hunks.entry(f.clone()).or_default().push(hunk);
                }
            } else if in_header && line == "--- /dev/null" {
                from_nothing = true;
            } else if in_header && let Some(path) = line.strip_prefix("+++ ") {
                file = path.strip_prefix("b/").map(str::to_string);
                if from_nothing && let Some(f) = &file {
                    scope.new.insert(f.clone());
                }
            }
        }
        scope
    }

    /// Whether the lines `start..=end` of `file` are touched by the branch.
    pub fn touches(&self, file: &str, start: u32, end: u32) -> bool {
        if self.new.contains(file) {
            return true;
        }
        self.hunks.get(file).is_some_and(|hunks| {
            hunks.iter().any(|&(from, count)| {
                if count == 0 {
                    // A removal between lines `from` and `from + 1`: inside
                    // the environment when both are.
                    start <= from && from < end
                } else {
                    from <= end && start < from + count
                }
            })
        })
    }

    /// Whether every line of `start..=end` of `file` is new on the branch.
    pub fn adds(&self, file: &str, start: u32, end: u32) -> bool {
        if self.new.contains(file) {
            return true;
        }
        self.hunks.get(file).is_some_and(|hunks| {
            (start..=end).all(|line| {
                hunks
                    .iter()
                    .any(|&(from, count)| from <= line && line < from + count)
            })
        })
    }

    /// Whether an environment of the library belongs to the branch.
    pub fn contains(&self, library: &Library, env: &Environment) -> bool {
        let (start, end) = env.record.lines();
        self.touches(&module_file(library, &env.record.module), start, end)
    }

    /// Whether an environment is new on the branch: all of it is.
    pub fn is_new(&self, library: &Library, env: &Environment) -> bool {
        let (start, end) = env.record.lines();
        self.adds(&module_file(library, &env.record.module), start, end)
    }
}

/// The current side of a hunk header, `-a,b +c,d @@`, as `(c, d)`.
fn parse_hunk(header: &str) -> Option<(u32, u32)> {
    let current = header.split_whitespace().find(|p| p.starts_with('+'))?;
    let current = current.strip_prefix('+')?;
    let (start, count) = match current.split_once(',') {
        Some((s, c)) => (s.parse().ok()?, c.parse().ok()?),
        None => (current.parse().ok()?, 1),
    };
    Some((start, count))
}

/// A module's file, relative to the library, as git names it.
fn module_file(library: &Library, module: &str) -> String {
    let path = library.module_path(module);
    path.strip_prefix(&library.dir)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Who last touched an environment: the author of the most recent commit
/// among those that wrote its lines, and the agent its trailer names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub agent: Option<String>,
}

impl Author {
    /// Who last touched the lines an environment spans (`git blame`), when
    /// they are committed.
    pub fn of(library: &Library, env: &Environment) -> Option<Self> {
        let (start, end) = env.record.lines();
        let file = module_file(library, &env.record.module);
        let blame = git(
            &library.dir,
            &[
                "blame",
                "--porcelain",
                "-L",
                &format!("{start},{end}"),
                "--",
                &file,
            ],
        )
        .ok()?;
        let (sha, name) = latest(&blame)?;
        let agent = git(
            &library.dir,
            &[
                "log",
                "-1",
                "--format=%(trailers:key=Agent,valueonly,separator=%x2C)",
                &sha,
            ],
        )
        .ok()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty());
        Some(Self { name, agent })
    }

    /// How it reads: `alice` or `alice (claude-opus-5-5)`.
    pub fn describe(&self) -> String {
        match &self.agent {
            Some(agent) => format!("{} ({agent})", self.name),
            None => self.name.clone(),
        }
    }
}

/// The most recent committed commit of a porcelain blame, with its author:
/// by commit time, and by hash between commits made in the same second.
fn latest(blame: &str) -> Option<(String, String)> {
    let mut commits: BTreeMap<String, (i64, String)> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in blame.lines() {
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            current = Some(first.to_string());
            commits.entry(first.to_string()).or_default();
        } else if let Some(sha) = &current {
            if let Some(name) = line.strip_prefix("author ") {
                commits.entry(sha.clone()).or_default().1 = name.to_string();
            } else if let Some(time) = line.strip_prefix("committer-time ") {
                commits.entry(sha.clone()).or_default().0 = time.trim().parse().unwrap_or(0);
            }
        }
    }
    commits
        .into_iter()
        .filter(|(sha, _)| sha.chars().any(|c| c != '0'))
        .max_by(|a, b| (a.1.0, &a.0).cmp(&(b.1.0, &b.0)))
        .map(|(sha, (_, name))| (sha, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
diff --git a/Alg/Even.lean b/Alg/Even.lean
index 1111111..2222222 100644
--- a/Alg/Even.lean
+++ b/Alg/Even.lean
@@ -10,0 +11,8 @@ some context
+added
@@ -30,2 +38 @@
-old
+new
@@ -50,3 +55,0 @@
-gone
--- /dev/null
+++ not a file
diff --git a/Alg/Odd.lean b/Alg/Odd.lean
new file mode 100644
--- /dev/null
+++ b/Alg/Odd.lean
@@ -0,0 +1,20 @@
+all new
";

    #[test]
    fn reads_the_lines_a_branch_changes() {
        let s = Scope::parse(DIFF);
        // Added lines 11 to 18.
        assert!(s.touches("Alg/Even.lean", 15, 25));
        assert!(s.touches("Alg/Even.lean", 1, 11));
        assert!(!s.touches("Alg/Even.lean", 1, 10));
        assert!(!s.touches("Alg/Even.lean", 19, 37));
        // A rewritten line, 38.
        assert!(s.touches("Alg/Even.lean", 38, 38));
        // Lines removed after line 55: inside 50..60, not at its edges.
        assert!(s.touches("Alg/Even.lean", 50, 60));
        assert!(!s.touches("Alg/Even.lean", 56, 60));
        assert!(!s.touches("Alg/Even.lean", 40, 55));
        // A new file belongs wholly to the branch.
        assert!(s.touches("Alg/Odd.lean", 100, 120));
        assert!(!s.touches("Alg/Other.lean", 1, 1000));
    }

    #[test]
    fn tells_new_environments_from_changed_ones() {
        let s = Scope::parse(DIFF);
        assert!(s.adds("Alg/Even.lean", 11, 18));
        assert!(!s.adds("Alg/Even.lean", 10, 18));
        assert!(!s.adds("Alg/Even.lean", 36, 40));
        assert!(s.adds("Alg/Odd.lean", 3, 9));
    }

    #[test]
    fn hunk_headers_without_a_count_are_one_line() {
        assert_eq!(parse_hunk("-3 +4 @@"), Some((4, 1)));
        assert_eq!(parse_hunk("-3,2 +4,0 @@ fn"), Some((4, 0)));
    }

    #[test]
    fn the_latest_commit_of_a_blame_wins() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let zero = "0".repeat(40);
        let blame = format!(
            "{a} 1 1 1\nauthor Alice\ncommitter-time 100\n\tline\n\
             {b} 2 2 1\nauthor Bob\ncommitter-time 200\n\tline\n\
             {a} 3 3\n\tline\n\
             {zero} 4 4 1\nauthor Not Committed Yet\ncommitter-time 300\n\tline\n"
        );
        assert_eq!(latest(&blame), Some((b, "Bob".to_string())));
    }
}
