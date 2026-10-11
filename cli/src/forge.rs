//! Everything `stemma` asks of the forge that hosts the group's repository:
//! who the person is there, finding and opening pull requests, and setting the
//! repository up. No other module talks to a forge.
//!
//! The library chooses its forge in `stemma.toml` (`[forge]`):
//!
//! - `github` (the default): GitHub, through its command line, `gh`;
//! - `forgejo`: a Forgejo instance (Codeberg, or a group's own), through its
//!   API, at `[forge] url`.
//!
//! `STEMMA_FORGE` overrides it:
//!
//! - `none`: no forge; pull requests are opened by hand;
//! - `file:<path>`: a forge kept in a JSON file, for tests and rehearsals. It
//!   records what it is asked to do, and never touches the network.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::ForgeKind;

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
/// writes (see [`workflow`]).
pub const REQUIRED_CHECKS: &[&str] = &["Stemma verify", "Stemma check"];

/// The workflow `stemma` writes for a kind of forge, relative to the library.
pub fn workflow(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::GitHub => ".github/workflows/stemma.yml",
        ForgeKind::Forgejo => ".forgejo/workflows/stemma.yml",
    }
}

/// The template of the workflow for a kind of forge.
pub fn workflow_template(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::GitHub => "library/github/stemma.yml",
        ForgeKind::Forgejo => "library/forgejo/stemma.yml",
    }
}

/// The directories of every forge's configuration, whichever the library
/// uses: changing them is a change of the policy, and agents may not.
pub const FORGE_DIRS: &[&str] = &[".github", ".forgejo"];

impl Step {
    /// Every step, in the order they are applied.
    pub const ALL: &'static [Step] = &[Step::Rules, Step::MergeCommits, Step::Pages];

    /// The steps a library needs: Pages only when it publishes its site, on
    /// GitHub.
    pub fn for_library(dir: &Path) -> Vec<Step> {
        let publish = crate::config::Config::read(dir)
            .is_ok_and(|c| c.site.publish && c.forge.kind == ForgeKind::GitHub);
        Self::ALL
            .iter()
            .copied()
            .filter(|s| *s != Step::Pages || publish)
            .collect()
    }

    /// What the step sets up, on a kind of forge.
    pub fn describe(self, kind: ForgeKind) -> &'static str {
        if kind == ForgeKind::Forgejo {
            return match self {
                Step::Rules => {
                    "`main` changes only through pull requests that pass the checks `Stemma \
                     verify` and `Stemma check`: no direct or forced pushes"
                }
                Step::MergeCommits => {
                    "pull requests are merged with merge commits only, and Actions are on"
                }
                Step::Pages => "Forgejo has no Pages: the site is not published",
            };
        }
        match self {
            Step::Rules => {
                "`main` changes only through pull requests that pass the checks `Stemma verify` \
                 and `Stemma check`: no direct or forced pushes"
            }
            Step::MergeCommits => "pull requests are merged with merge commits only",
            Step::Pages => "GitHub Pages publishes the site the workflow builds on `main`",
        }
    }

    /// How a person sets it up by hand, on a kind of forge.
    pub fn by_hand(self, kind: ForgeKind) -> &'static str {
        if kind == ForgeKind::Forgejo {
            return match self {
                Step::Rules => {
                    "in the repository's Settings → Branches, add a protection rule for `main` \
                     that disables pushing, enables status checks with the patterns \
                     \"Stemma / Stemma verify *\" and \"Stemma / Stemma check *\", and requires \
                     no approvals"
                }
                Step::MergeCommits => {
                    "in the repository's Settings, enable Actions (Units), and under Pull \
                     Requests allow only \"Create merge commit\" as the merge style"
                }
                Step::Pages => "nothing: the site is not published",
            };
        }
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
    /// The kind of forge, for what is said to people.
    fn kind(&self) -> ForgeKind {
        ForgeKind::GitHub
    }

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

/// The forge of the library at `dir`, or of `STEMMA_FORGE` (see the module's
/// documentation). Outside a library, GitHub.
pub fn current(dir: &Path) -> Box<dyn Forge> {
    let config = crate::library::Library::find(dir)
        .map(|l| l.config.forge)
        .unwrap_or_default();
    of(&config)
}

/// The forge a `[forge]` table names, or `STEMMA_FORGE`.
pub fn of(config: &crate::config::Forge) -> Box<dyn Forge> {
    match std::env::var(FORGE_VARIABLE).as_deref() {
        Ok("none") => return Box::new(NoForge),
        Ok(other) if other.starts_with("file:") => {
            return Box::new(FileForge {
                path: PathBuf::from(&other["file:".len()..]),
            });
        }
        _ => {}
    }
    match config.kind {
        ForgeKind::GitHub => Box::new(GitHub),
        ForgeKind::Forgejo => Box::new(Forgejo {
            url: config.url.clone(),
        }),
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
pub fn report(forge: &dyn Forge, outcomes: &[Outcome], say: impl Fn(bool, &str)) {
    let kind = forge.kind();
    for o in outcomes {
        match &o.error {
            None => say(
                true,
                &format!("Set the repository up: {}.", o.step.describe(kind)),
            ),
            Some(error) => say(
                false,
                &format!(
                    "Could not set the repository up ({error}): {}. Set it by hand: {}.",
                    o.step.describe(kind),
                    o.step.by_hand(kind)
                ),
            ),
        }
    }
}

// GitHub, through `gh`.

struct GitHub;

/// The host and `owner/name` of a repository, from its URL on any forge:
/// `git@host:owner/name.git`, `ssh://git@host:2222/owner/name.git` or
/// `https://host/owner/name`.
fn remote_parts(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?,
        None => url.split_once(':')?,
    };
    let host = authority.rsplit('@').next()?.split(':').next()?;
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && !host.is_empty() && !owner.is_empty() && !name.is_empty())
        .then(|| (host.to_string(), format!("{owner}/{name}")))
}

/// `owner/name` from the URL of a GitHub repository.
fn github_slug(url: &str) -> Option<String> {
    remote_parts(url)
        .filter(|(host, _)| host == "github.com")
        .map(|(_, slug)| slug)
}

/// The URL of `origin`.
fn origin(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!("the library has no remote `origin`");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The group's repository on GitHub: the one `origin` points at.
fn repository(dir: &Path) -> Result<String> {
    let url = origin(dir)?;
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

// Forgejo, through its API.

/// The variable that holds a token of the Forgejo instance, with the scopes
/// `read:user` and `write:repository`.
pub const FORGEJO_TOKEN: &str = "STEMMA_FORGEJO_TOKEN";

struct Forgejo {
    /// The instance, from `[forge] url`.
    url: Option<String>,
}

/// The status checks the protection of `main` requires: Forgejo names them
/// `<workflow> / <job> (<event>)`.
fn forgejo_checks() -> Vec<String> {
    REQUIRED_CHECKS
        .iter()
        .map(|c| format!("Stemma / {c} *"))
        .collect()
}

/// The protection of `main`: no pushes, so changes come through pull
/// requests, which pass the checks. Forgejo refuses forced pushes and
/// deletion of a protected branch by itself.
fn branch_protection() -> serde_json::Value {
    json!({
        "enable_push": false,
        "enable_status_check": true,
        "status_check_contexts": forgejo_checks(),
        "required_approvals": 0,
        // A branch need not be up to date with `main`.
        "block_on_outdated_branch": false,
    })
}

/// The settings of the repository: merge commits only, and Actions on.
fn repository_settings() -> serde_json::Value {
    json!({
        "has_actions": true,
        "has_pull_requests": true,
        "allow_merge_commits": true,
        "allow_squash_merge": false,
        "allow_rebase": false,
        "allow_rebase_explicit": false,
        "allow_fast_forward_only_merge": false,
        "default_merge_style": "merge",
    })
}

/// A pull request as Forgejo's API gives it.
#[derive(Deserialize)]
struct ForgejoPullRequest {
    number: u64,
    html_url: String,
    state: String,
    title: String,
    #[serde(default)]
    merged: bool,
    head: ForgejoBranch,
    base: ForgejoBranch,
}

#[derive(Deserialize)]
struct ForgejoBranch {
    #[serde(rename = "ref")]
    name: String,
    repo: Option<ForgejoRepository>,
}

#[derive(Deserialize)]
struct ForgejoRepository {
    full_name: String,
}

impl ForgejoPullRequest {
    /// Whether it comes from a branch of `repo` itself, not from a fork.
    fn is_from(&self, repo: &str) -> bool {
        self.head
            .repo
            .as_ref()
            .is_some_and(|r| r.full_name.eq_ignore_ascii_case(repo))
    }

    fn into_pull_request(self) -> PullRequest {
        PullRequest {
            number: self.number,
            url: self.html_url,
            head: self.head.name,
            base: self.base.name,
            state: match (self.merged, self.state.as_str()) {
                (true, _) => State::Merged,
                (false, "open") => State::Open,
                _ => State::Closed,
            },
            title: self.title,
        }
    }
}

/// Base64, for HTTP's basic authentication.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

impl Forgejo {
    /// The instance's address, without a trailing slash.
    fn base(&self) -> Result<&str> {
        self.url
            .as_deref()
            .map(|u| u.trim().trim_end_matches('/'))
            .filter(|u| !u.is_empty())
            .ok_or_else(|| {
                anyhow!("stemma.toml names no Forgejo instance: `[forge] url` is missing")
            })
    }

    /// The `Authorization` header: the token of `STEMMA_FORGEJO_TOKEN`, or
    /// the credentials git keeps for the instance, without asking for them.
    fn authorization(&self) -> Option<String> {
        if let Ok(token) = std::env::var(FORGEJO_TOKEN)
            && !token.trim().is_empty()
        {
            return Some(format!("token {}", token.trim()));
        }
        let (protocol, rest) = self.base().ok()?.split_once("://")?;
        let host = rest.split('/').next()?;
        let mut child = Command::new("git")
            .args(["credential", "fill"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        {
            use std::io::Write;
            let mut stdin = child.stdin.take()?;
            write!(stdin, "protocol={protocol}\nhost={host}\n\n").ok()?;
        }
        let out = child.wait_with_output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{key}=")))
                .map(str::to_string)
        };
        let (user, password) = (field("username")?, field("password")?);
        Some(format!(
            "Basic {}",
            base64(format!("{user}:{password}").as_bytes())
        ))
    }

    /// Calls the API: `GET` without a body, `POST` or `PATCH` with one. It
    /// gives the status and the answer, whatever the status.
    fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(u16, serde_json::Value)> {
        let base = self.base()?;
        let url = format!("{base}/api/v1{path}");
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .tls_config(tls)
            .build()
            .into();
        let authorization = self.authorization();
        fn headers<B>(
            request: ureq::RequestBuilder<B>,
            authorization: Option<&str>,
        ) -> ureq::RequestBuilder<B> {
            let request = request.header("Accept", "application/json");
            match authorization {
                Some(a) => request.header("Authorization", a),
                None => request,
            }
        }
        let auth = authorization.as_deref();
        let response = match (method, body) {
            ("GET", None) => headers(agent.get(&url), auth).call(),
            ("POST" | "PATCH", Some(body)) => {
                let request = if method == "POST" {
                    agent.post(&url)
                } else {
                    agent.patch(&url)
                };
                headers(request, auth)
                    .header("Content-Type", "application/json")
                    .send(body.to_string())
            }
            _ => bail!("{method} {path}: not a call stemma makes"),
        };
        let mut response = response.with_context(|| format!("reaching {base}"))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok((status, value))
    }

    /// Calls the API, failing unless the status is a success.
    fn ok(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let (status, value) = self.call(method, path, body)?;
        if (200..300).contains(&status) {
            return Ok(value);
        }
        let message = value
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("no explanation");
        let hint = if matches!(status, 401 | 403) && self.authorization().is_none() {
            format!(
                "; set {FORGEJO_TOKEN} to a token of {} with the scopes read:user and \
                 write:repository",
                self.base().unwrap_or_default()
            )
        } else {
            String::new()
        };
        bail!("Forgejo answered {status} to {method} {path}: {message}{hint}")
    }

    /// The group's repository: the one `origin` points at, as `owner/name`.
    fn repository(&self, dir: &Path) -> Result<String> {
        let url = origin(dir)?;
        remote_parts(&url)
            .map(|(_, slug)| slug)
            .ok_or_else(|| anyhow!("`origin` ({url}) is not the URL of a repository"))
    }
}

impl Forge for Forgejo {
    fn kind(&self) -> ForgeKind {
        ForgeKind::Forgejo
    }

    fn login(&self) -> Option<String> {
        self.authorization()?;
        self.ok("GET", "/user", None)
            .ok()?
            .get("login")?
            .as_str()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
    }

    fn pull_requests(&self, dir: &Path, heads: &[&str]) -> Result<Vec<PullRequest>> {
        let repo = self.repository(dir)?;
        let mut all = Vec::new();
        // The 200 updated most recently, as for GitHub.
        for page in 1..=4 {
            let value = self.ok(
                "GET",
                &format!("/repos/{repo}/pulls?state=all&sort=recentupdate&limit=50&page={page}"),
                None,
            )?;
            let prs: Vec<ForgejoPullRequest> =
                serde_json::from_value(value).context("reading the pull requests Forgejo lists")?;
            let last = prs.len() < 50;
            all.extend(prs);
            if last {
                break;
            }
        }
        let mut found: Vec<PullRequest> = all
            .into_iter()
            .filter(|pr| pr.is_from(&repo) && heads.contains(&pr.head.name.as_str()))
            .map(ForgejoPullRequest::into_pull_request)
            .collect();
        found.sort_by_key(|pr| std::cmp::Reverse(pr.number));
        Ok(found)
    }

    fn open_pull_request(&self, dir: &Path, new: &NewPullRequest) -> Result<PullRequest> {
        let repo = self.repository(dir)?;
        let body = json!({
            "head": new.head, "base": new.base, "title": new.title, "body": new.body,
        });
        let value = self.ok("POST", &format!("/repos/{repo}/pulls"), Some(&body))?;
        let pr: ForgejoPullRequest =
            serde_json::from_value(value).context("reading the pull request Forgejo opened")?;
        Ok(pr.into_pull_request())
    }

    fn apply(&self, dir: &Path, step: Step) -> Result<()> {
        let repo = self.repository(dir)?;
        match step {
            Step::Rules => {
                let path = format!("/repos/{repo}/branch_protections/main");
                let (status, _) = self.call("GET", &path, None)?;
                if status == 404 {
                    let mut body = branch_protection();
                    body["rule_name"] = json!("main");
                    self.ok(
                        "POST",
                        &format!("/repos/{repo}/branch_protections"),
                        Some(&body),
                    )?;
                } else {
                    self.ok("PATCH", &path, Some(&branch_protection()))?;
                }
            }
            Step::MergeCommits => {
                self.ok(
                    "PATCH",
                    &format!("/repos/{repo}"),
                    Some(&repository_settings()),
                )?;
            }
            Step::Pages => bail!("Forgejo has no Pages"),
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
    fn reads_remote_urls_of_any_forge() {
        for (url, host) in [
            ("git@codeberg.org:group/algebra.git", "codeberg.org"),
            (
                "ssh://git@git.uni.edu:2222/group/algebra.git",
                "git.uni.edu",
            ),
            ("https://git.uni.edu/group/algebra/", "git.uni.edu"),
            (
                "https://user@codeberg.org/group/algebra.git",
                "codeberg.org",
            ),
        ] {
            assert_eq!(
                remote_parts(url),
                Some((host.to_string(), "group/algebra".to_string())),
                "{url}"
            );
        }
        assert_eq!(remote_parts("/srv/git/algebra.git"), None);
        assert_eq!(remote_parts("https://codeberg.org/algebra"), None);
    }

    #[test]
    fn forgejo_protects_main_and_merges_with_merge_commits() {
        let protection = branch_protection();
        assert_eq!(protection["enable_push"], false);
        assert_eq!(
            protection["status_check_contexts"],
            json!(["Stemma / Stemma verify *", "Stemma / Stemma check *"])
        );
        let settings = repository_settings();
        assert_eq!(settings["allow_merge_commits"], true);
        assert_eq!(settings["allow_squash_merge"], false);
        assert_eq!(settings["default_merge_style"], "merge");
    }

    #[test]
    fn reads_forgejo_pull_requests() {
        let pr = |merged: bool, state: &str, owner: &str| {
            serde_json::from_value::<ForgejoPullRequest>(json!({
                "number": 7, "html_url": "https://codeberg.org/g/a/pulls/7", "state": state,
                "title": "T", "merged": merged,
                "head": { "ref": "share/alice", "repo": { "full_name": format!("{owner}/a") } },
                "base": { "ref": "main", "repo": { "full_name": "g/a" } },
            }))
            .unwrap()
        };
        assert!(pr(false, "open", "G").is_from("g/a"));
        assert!(!pr(false, "open", "fork").is_from("g/a"));
        let open = pr(false, "open", "g").into_pull_request();
        assert_eq!(
            (open.state, open.head.as_str()),
            (State::Open, "share/alice")
        );
        assert_eq!(
            pr(true, "closed", "g").into_pull_request().state,
            State::Merged
        );
        assert_eq!(
            pr(false, "closed", "g").into_pull_request().state,
            State::Closed
        );
    }

    /// Against Codeberg, which runs Forgejo: `cargo test -- --ignored`.
    #[test]
    #[ignore = "reaches codeberg.org"]
    fn lists_pull_requests_on_codeberg() {
        let dir = std::env::temp_dir().join(format!("stemma-codeberg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &[
                "remote",
                "add",
                "origin",
                "https://codeberg.org/forgejo/forgejo.git",
            ],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&dir)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let forge = Forgejo {
            url: Some("https://codeberg.org/".into()),
        };
        let all = forge.pull_requests(&dir, &[]).unwrap();
        assert!(all.is_empty());
        let (status, value) = forge
            .call(
                "GET",
                "/repos/forgejo/forgejo/pulls?state=all&limit=1",
                None,
            )
            .unwrap();
        assert_eq!(status, 200, "{value}");
        let prs: Vec<ForgejoPullRequest> = serde_json::from_value(value).unwrap();
        let pr = prs.into_iter().next().unwrap();
        assert!(pr.is_from("forgejo/forgejo") || pr.head.repo.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b""), "");
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
