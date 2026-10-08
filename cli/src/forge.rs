//! Everything `stemma` asks of the forge that hosts the group's repository:
//! who the person is there, finding and opening pull requests, and setting the
//! repository up. No other module talks to a forge.
//!
//! The forge is chosen by `STEMMA_FORGE`:
//!
//! - `github` (the default): GitHub, through its command line, `gh`;
//! - `none`: no forge; pull requests are opened by hand;
//! - `file:<path>`: a forge kept in a JSON file, for tests and rehearsals. It
//!   records what it is asked to do, and never touches the network.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The variable that chooses the forge.
pub const FORGE_VARIABLE: &str = "STEMMA_FORGE";

/// The state of a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Open,
    Merged,
    Closed,
}

/// A pull request, as the forge knows it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    /// The branch it comes from.
    pub head: String,
    /// The branch it goes to.
    pub base: String,
    pub state: State,
    pub title: String,
}

/// A pull request to open.
pub struct NewPullRequest<'a> {
    pub head: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
}

/// A step of setting the group's repository up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    /// A ruleset on `main`: pull requests required, no direct or forced
    /// pushes, no deletion, and the checks of the workflow required.
    Rules,
    /// Pull requests are merged with merge commits only.
    MergeCommits,
    /// GitHub Pages publishes the site the workflow builds on `main`. Only
    /// for a library that publishes its site (`[site] publish`).
    Pages,
}

/// The names of the required checks: the jobs of the workflow `stemma`
/// writes (`.github/workflows/stemma.yml`).
pub const REQUIRED_CHECKS: &[&str] = &["Stemma verify", "Stemma check"];

impl Step {
    /// Every step, in the order they are applied.
    pub const ALL: &'static [Step] = &[Step::Rules, Step::MergeCommits, Step::Pages];

    /// The steps a library needs: Pages only when it publishes its site.
    pub fn for_library(dir: &Path) -> Vec<Step> {
        let publish = crate::config::Config::read(dir).is_ok_and(|c| c.site.publish);
        Self::ALL
            .iter()
            .copied()
            .filter(|s| *s != Step::Pages || publish)
            .collect()
    }

    /// What the step sets up.
    pub fn describe(self) -> &'static str {
        match self {
            Step::Rules => {
                "`main` changes only through pull requests that pass the checks `Stemma verify` \
                 and `Stemma check`: no direct or forced pushes"
            }
            Step::MergeCommits => "pull requests are merged with merge commits only",
            Step::Pages => "GitHub Pages publishes the site the workflow builds on `main`",
        }
    }

    /// How a person sets it up by hand, on GitHub.
    pub fn by_hand(self) -> &'static str {
        match self {
            Step::Rules => {
                "in the repository's Settings → Rules → Rulesets, add a ruleset named \
                 \"Stemma\" for the branch `main` that restricts deletions, requires a pull \
                 request before merging (no approvals needed), requires the status checks \
                 \"Stemma verify\" and \"Stemma check\", and blocks force pushes"
            }
            Step::MergeCommits => {
                "in the repository's Settings → General → Pull Requests, allow merge commits, \
                 and disallow squash merging and rebase merging"
            }
            Step::Pages => {
                "in the repository's Settings → Pages, set the source to \"GitHub Actions\" \
                 (GitHub Pages needs a public repository, or a plan that allows it for private \
                 ones); until then, the workflow's job \"Publish the site\" fails, and nothing \
                 else does"
            }
        }
    }
}

/// What the forge did with a step: `None` when it was set up, or why not.
#[derive(Debug, Serialize)]
pub struct Outcome {
    pub step: Step,
    pub error: Option<String>,
}

/// What `stemma` asks of a forge.
pub trait Forge {
    /// The person's account on the forge, when it is known.
    fn login(&self) -> Option<String>;

    /// The pull requests of the group's repository, open or not, that come
    /// from one of `heads`, newest first.
    fn pull_requests(&self, dir: &Path, heads: &[&str]) -> Result<Vec<PullRequest>>;

    /// Opens a pull request.
    fn open_pull_request(&self, dir: &Path, new: &NewPullRequest) -> Result<PullRequest>;

    /// Applies one step of the repository's setup. It must be idempotent.
    fn apply(&self, dir: &Path, step: Step) -> Result<()>;
}

/// The forge of this process (see the module's documentation).
pub fn current() -> Box<dyn Forge> {
    match std::env::var(FORGE_VARIABLE).as_deref() {
        Ok("none") => Box::new(NoForge),
        Ok(other) if other.starts_with("file:") => Box::new(FileForge {
            path: PathBuf::from(&other["file:".len()..]),
        }),
        _ => Box::new(GitHub),
    }
}

/// Sets the group's repository up, step by step. A step that fails does not
/// stop the others; its outcome says why.
pub fn set_up(forge: &dyn Forge, dir: &Path) -> Vec<Outcome> {
    Step::for_library(dir)
        .into_iter()
        .map(|step| Outcome {
            step,
            error: forge.apply(dir, step).err().map(|e| format!("{e:#}")),
        })
        .collect()
}

/// Says what setting the repository up did, and what to set by hand, through
/// `say(ok, text)`.
pub fn report(outcomes: &[Outcome], say: impl Fn(bool, &str)) {
    for o in outcomes {
        match &o.error {
            None => say(
                true,
                &format!("Set the repository up: {}.", o.step.describe()),
            ),
            Some(error) => say(
                false,
                &format!(
                    "Could not set the repository up ({error}): {}. Set it by hand: {}.",
                    o.step.describe(),
                    o.step.by_hand()
                ),
            ),
        }
    }
}

// GitHub, through `gh`.

struct GitHub;

/// `owner/name` from the URL of a GitHub repository.
fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && !owner.is_empty() && !name.is_empty())
        .then(|| format!("{owner}/{name}"))
}

/// The group's repository on GitHub: the one `origin` points at.
fn repository(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!("the library has no remote `origin`");
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    github_slug(&url).ok_or_else(|| anyhow!("`origin` ({url}) is not a repository on GitHub"))
}

/// Runs `gh`, with `input` on its stdin, and returns its output.
fn gh(dir: &Path, args: &[&str], input: Option<&str>) -> Result<String> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .current_dir(dir)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| anyhow!("`gh`, GitHub's command line, is not installed"))?;
    if let Some(input) = input {
        use std::io::Write;
        child
            .stdin
            .take()
            .context("writing to gh")?
            .write_all(input.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let error = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(
            "{}",
            if error.is_empty() {
                "`gh` failed".to_string()
            } else {
                error
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A pull request as `gh pr list --json` gives it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    url: String,
    head_ref_name: String,
    base_ref_name: String,
    state: String,
    title: String,
    #[serde(default)]
    is_cross_repository: bool,
}

/// The ruleset on `main`.
fn ruleset() -> serde_json::Value {
    json!({
        "name": "Stemma",
        "target": "branch",
        "enforcement": "active",
        "conditions": { "ref_name": { "include": ["refs/heads/main"], "exclude": [] } },
        "rules": [
            { "type": "deletion" },
            { "type": "non_fast_forward" },
            { "type": "pull_request", "parameters": {
                "required_approving_review_count": 0,
                "dismiss_stale_reviews_on_push": false,
                "require_code_owner_review": false,
                "require_last_push_approval": false,
                "required_review_thread_resolution": false,
                "allowed_merge_methods": ["merge"],
            } },
            // Not strict: a branch need not be up to date with `main`, which
            // GitHub would bring in with a commit of its own on the branch.
            { "type": "required_status_checks", "parameters": {
                "strict_required_status_checks_policy": false,
                "required_status_checks": REQUIRED_CHECKS
                    .iter()
                    .map(|c| json!({ "context": c }))
                    .collect::<Vec<_>>(),
            } },
        ],
    })
}

impl Forge for GitHub {
    fn login(&self) -> Option<String> {
        gh(Path::new("."), &["api", "user", "--jq", ".login"], None)
            .ok()
            .filter(|l| !l.is_empty())
    }

    fn pull_requests(&self, dir: &Path, heads: &[&str]) -> Result<Vec<PullRequest>> {
        let repo = repository(dir)?;
        let out = gh(
            dir,
            &[
                "pr",
                "list",
                "--repo",
                &repo,
                "--state",
                "all",
                "--limit",
                "200",
                "--json",
                "number,url,headRefName,baseRefName,state,title,isCrossRepository",
            ],
            None,
        )?;
        let all: Vec<GhPullRequest> =
            serde_json::from_str(&out).context("reading the pull requests `gh` lists")?;
        Ok(all
            .into_iter()
            .filter(|pr| !pr.is_cross_repository && heads.contains(&pr.head_ref_name.as_str()))
            .map(|pr| PullRequest {
                number: pr.number,
                url: pr.url,
                head: pr.head_ref_name,
                base: pr.base_ref_name,
                state: match pr.state.as_str() {
                    "OPEN" => State::Open,
                    "MERGED" => State::Merged,
                    _ => State::Closed,
                },
                title: pr.title,
            })
            .collect())
    }

    fn open_pull_request(&self, dir: &Path, new: &NewPullRequest) -> Result<PullRequest> {
        let repo = repository(dir)?;
        let url = gh(
            dir,
            &[
                "pr", "create", "--repo", &repo, "--base", new.base, "--head", new.head, "--title",
                new.title, "--body", new.body,
            ],
            None,
        )?;
        let url = url.lines().last().unwrap_or_default().to_string();
        let number = url
            .rsplit('/')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or_default();
        Ok(PullRequest {
            number,
            url,
            head: new.head.into(),
            base: new.base.into(),
            state: State::Open,
            title: new.title.into(),
        })
    }

    fn apply(&self, dir: &Path, step: Step) -> Result<()> {
        let repo = repository(dir)?;
        match step {
            Step::Rules => {
                let existing = gh(
                    dir,
                    &[
                        "api",
                        &format!("repos/{repo}/rulesets"),
                        "--jq",
                        ".[] | select(.name == \"Stemma\") | .id",
                    ],
                    None,
                )?;
                let body = ruleset().to_string();
                match existing.lines().next() {
                    Some(id) => gh(
                        dir,
                        &[
                            "api",
                            "--method",
                            "PUT",
                            &format!("repos/{repo}/rulesets/{id}"),
                            "--input",
                            "-",
                        ],
                        Some(&body),
                    )?,
                    None => gh(
                        dir,
                        &[
                            "api",
                            "--method",
                            "POST",
                            &format!("repos/{repo}/rulesets"),
                            "--input",
                            "-",
                        ],
                        Some(&body),
                    )?,
                };
            }
            Step::MergeCommits => {
                gh(
                    dir,
                    &[
                        "api",
                        "--method",
                        "PATCH",
                        &format!("repos/{repo}"),
                        "-F",
                        "allow_merge_commit=true",
                        "-F",
                        "allow_squash_merge=false",
                        "-F",
                        "allow_rebase_merge=false",
                    ],
                    None,
                )?;
            }
            Step::Pages => {
                // Built by a workflow, not from a branch; created if missing.
                let exists = gh(dir, &["api", &format!("repos/{repo}/pages")], None).is_ok();
                let method = if exists { "PUT" } else { "POST" };
                gh(
                    dir,
                    &[
                        "api",
                        "--method",
                        method,
                        &format!("repos/{repo}/pages"),
                        "-f",
                        "build_type=workflow",
                    ],
                    None,
                )?;
            }
        }
        Ok(())
    }
}

// No forge.

struct NoForge;

impl Forge for NoForge {
    fn login(&self) -> Option<String> {
        None
    }

    fn pull_requests(&self, _: &Path, _: &[&str]) -> Result<Vec<PullRequest>> {
        Ok(Vec::new())
    }

    fn open_pull_request(&self, _: &Path, new: &NewPullRequest) -> Result<PullRequest> {
        bail!(
            "no forge is configured: open a pull request from {} to {} by hand",
            new.head,
            new.base
        )
    }

    fn apply(&self, _: &Path, _: Step) -> Result<()> {
        bail!("no forge is configured")
    }
}

// A forge kept in a file.

/// What a [`FileForge`] holds.
#[derive(Default, Serialize, Deserialize)]
struct FileState {
    #[serde(default)]
    login: Option<String>,
    #[serde(default)]
    pull_requests: Vec<PullRequest>,
    /// The steps of the setup applied, in order.
    #[serde(default)]
    setup: Vec<Step>,
    /// Refuse to set the repository up, as a forge does without permission.
    #[serde(default)]
    deny_setup: bool,
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Step::ALL
            .iter()
            .copied()
            .find(|s| serde_json::to_value(s).is_ok_and(|v| v == name.as_str()))
            .ok_or_else(|| serde::de::Error::custom(format!("unknown step {name}")))
    }
}

struct FileForge {
    path: PathBuf,
}

impl FileForge {
    fn read(&self) -> Result<FileState> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("reading {}", self.path.display())),
            Err(_) => Ok(FileState::default()),
        }
    }

    fn write(&self, state: &FileState) -> Result<()> {
        std::fs::write(&self.path, serde_json::to_string_pretty(state)?)
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

impl Forge for FileForge {
    fn login(&self) -> Option<String> {
        self.read().ok()?.login
    }

    fn pull_requests(&self, _: &Path, heads: &[&str]) -> Result<Vec<PullRequest>> {
        let mut prs: Vec<PullRequest> = self
            .read()?
            .pull_requests
            .into_iter()
            .filter(|pr| heads.contains(&pr.head.as_str()))
            .collect();
        prs.reverse();
        Ok(prs)
    }

    fn open_pull_request(&self, _: &Path, new: &NewPullRequest) -> Result<PullRequest> {
        let mut state = self.read()?;
        let number = state.pull_requests.len() as u64 + 1;
        let pr = PullRequest {
            number,
            url: format!("file://{}#{number}", self.path.display()),
            head: new.head.into(),
            base: new.base.into(),
            state: State::Open,
            title: new.title.into(),
        };
        state.pull_requests.push(pr.clone());
        self.write(&state)?;
        Ok(pr)
    }

    fn apply(&self, _: &Path, step: Step) -> Result<()> {
        let mut state = self.read()?;
        if state.deny_setup {
            bail!("HTTP 403: Resource not accessible by integration");
        }
        if !state.setup.contains(&step) {
            state.setup.push(step);
        }
        self.write(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_github_urls() {
        for url in [
            "git@github.com:group/algebra.git",
            "https://github.com/group/algebra",
            "https://github.com/group/algebra.git/",
            "ssh://git@github.com/group/algebra.git",
        ] {
            assert_eq!(github_slug(url).as_deref(), Some("group/algebra"), "{url}");
        }
        assert_eq!(github_slug("/srv/git/algebra.git"), None);
        assert_eq!(github_slug("https://gitlab.com/group/algebra"), None);
    }

    #[test]
    fn the_ruleset_requires_the_check_and_merge_commits() {
        let rules = ruleset().to_string();
        assert!(rules.contains("\"context\":\"Stemma verify\""));
        assert!(rules.contains("\"context\":\"Stemma check\""));
        assert!(rules.contains("\"allowed_merge_methods\":[\"merge\"]"));
        assert!(rules.contains("non_fast_forward"));
    }

    #[test]
    fn a_file_forge_records_what_it_is_asked() {
        let path = std::env::temp_dir().join(format!("stemma-forge-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let forge = FileForge { path: path.clone() };
        let dir = Path::new(".");
        let pr = forge
            .open_pull_request(
                dir,
                &NewPullRequest {
                    head: "share/alice",
                    base: "main",
                    title: "T",
                    body: "",
                },
            )
            .unwrap();
        assert_eq!(pr.number, 1);
        let found = forge.pull_requests(dir, &["share/alice"]).unwrap();
        assert_eq!(found[0].number, 1);
        assert_eq!(found[0].state, State::Open);
        assert!(forge.pull_requests(dir, &["share/bob"]).unwrap().is_empty());
        let outcomes = set_up(&forge, dir);
        assert!(outcomes.iter().all(|o| o.error.is_none()));
        set_up(&forge, dir);
        // Without a library that publishes its site, no Pages.
        assert_eq!(
            forge.read().unwrap().setup,
            [Step::Rules, Step::MergeCommits]
        );
        let _ = std::fs::remove_file(&path);
    }
}
