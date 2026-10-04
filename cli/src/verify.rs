//! `stemma verify`: what a change must carry before it reaches `main`, beyond
//! `stemma check`: signatures and approvals.
//!
//! Everything it checks is in git: who signed is the member whose key made a
//! commit's signature, checked by git against the keys in `stemma.toml`.
//! The members and the policy are read from the base, never from the change,
//! so that a change cannot alter what judges it. It gives the same answer on
//! any machine and on any forge.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::commands;
use crate::config::Config;
use crate::keys::Verifier;
use crate::library::Library;
use crate::report::{Report, Signature};
use crate::ui;

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
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Something a change lacks.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Missing {
    /// A signed environment is stale: a signer must sign it again.
    Signature { label: String },
    /// A signature applies to no central environment any more (it was
    /// removed, made not central, or relabelled): a signer must withdraw it.
    Withdrawal { label: String },
    /// A kind of change needs an approval from a member with one of `roles`.
    Approval { change: String, roles: Vec<String> },
    /// Something is wrong that no signature or approval fixes.
    Problem { message: String },
}

impl Missing {
    pub fn describe(&self) -> String {
        match self {
            Missing::Signature { label } => {
                format!("The signature of '{label}' is stale: it must be signed again.")
            }
            Missing::Withdrawal { label } => format!(
                "The signature of '{label}' applies to no central environment: a signer must \
                 withdraw it."
            ),
            Missing::Approval { change, roles } => format!(
                "A change of kind '{change}' needs the approval of a member with one of \
                 these roles: {}.",
                roles.join(", ")
            ),
            Missing::Problem { message } => message.clone(),
        }
    }

    /// Whether `config`'s `member` can supply it, by signing or approving.
    pub fn can_supply(&self, config: &Config, member: &str) -> bool {
        match self {
            Missing::Signature { .. } | Missing::Withdrawal { .. } => {
                config.has_role(member, &["signer".into()])
            }
            Missing::Approval { roles, .. } => config.has_role(member, roles),
            Missing::Problem { .. } => false,
        }
    }
}

/// What a change carries and lacks.
pub struct Verdict {
    /// The members and policy of the base.
    pub base_config: Config,
    pub missing: Vec<Missing>,
    pub notes: Vec<String>,
    /// The kinds of change, and the members who validly approved each.
    pub approvals: BTreeMap<String, BTreeSet<String>>,
}

/// What a signature file says about its signer.
#[derive(Deserialize)]
struct SignatureSigner {
    signer: String,
}

/// The kinds of change from `base` to HEAD that the policy may ask approvals
/// for, from the files the change touches.
pub fn kinds(dir: &Path, base: &str) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let changed = git(dir, &["diff", "--name-only", &format!("{base}...HEAD")])?;
    for file in changed.lines() {
        if file == "stemma.toml" || file.starts_with(".github/") {
            out.insert("policy".to_string());
        }
        if matches!(
            file,
            "lakefile.toml" | "lake-manifest.json" | "lean-toolchain"
        ) {
            out.insert("dependencies".to_string());
        }
    }
    Ok(out)
}

/// The content of a revision: a digest of every file in it but signatures,
/// made by git. It is what an approval approves: signatures added after it, or
/// with it, do not change it; any other change does.
pub fn content_id(dir: &Path, rev: &str) -> Result<String> {
    let listing = git(dir, &["ls-tree", "-r", "--full-tree", rev])?;
    let content: String = listing
        .lines()
        .filter(|line| {
            line.split_once('\t')
                .is_none_or(|(_, path)| !path.starts_with("signatures/"))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .context("writing to git")?
            .write_all(content.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    anyhow::ensure!(out.status.success(), "`git hash-object` failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The value of a commit's trailer, if it has one.
fn trailer(dir: &Path, sha: &str, key: &str) -> Result<String> {
    git(
        dir,
        &[
            "log",
            "-1",
            &format!("--format=%(trailers:key={key},valueonly,separator=%x2C)"),
            sha,
        ],
    )
}

/// The files a commit changes, against its first parent (for a merge, what it
/// brings in).
fn files_of(dir: &Path, sha: &str) -> Result<Vec<String>> {
    let parent = format!("{sha}^");
    let text = git(dir, &["diff", "--name-only", &parent, sha])?;
    Ok(text.lines().map(str::to_string).collect())
}

/// The kinds a commit approves, from its `Approve:` trailers.
fn approved_kinds(dir: &Path, sha: &str) -> Result<Vec<String>> {
    Ok(trailer(dir, sha, "Approve")?
        .split(',')
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect())
}

/// Works out what the change from `base` to HEAD carries and lacks. `report`
/// is the library's report, when it was built: without it, stale signatures
/// and new central environments are not looked at.
pub fn evaluate(library: &Library, base: &str, report: Option<&Report>) -> Result<Verdict> {
    let dir = &library.dir;
    let base_config = match git(dir, &["show", &format!("{base}:stemma.toml")]) {
        Ok(text) => Config::parse(&text).context("reading stemma.toml at the base")?,
        Err(_) => Config::read(dir)?,
    };
    let verifier = Verifier::new(&base_config)?;
    let mut missing = Vec::new();
    let mut notes = Vec::new();

    match report {
        Some(report) => {
            for env in &report.environments {
                if env.signature(dir) == Some(Signature::Stale) {
                    missing.push(Missing::Signature {
                        label: env.record.label.clone().unwrap_or_default(),
                    });
                }
            }
            // Every signature must apply to a central environment.
            let central: BTreeSet<&str> = report
                .environments
                .iter()
                .filter(|e| e.is_central_claim())
                .filter_map(|e| e.record.label.as_deref())
                .collect();
            for label in signature_labels(dir)? {
                if !central.contains(label.as_str()) {
                    missing.push(Missing::Withdrawal { label });
                }
            }
        }
        None => notes.push(
            "The library was not built: stale and withdrawn signatures were not looked at.".into(),
        ),
    }

    // Signature files come in commits of their own, signed by a signer.
    // Approvals are signed commits that touch nothing but signatures, and name
    // the content they approve: they count only if it is the content now.
    let current = content_id(dir, "HEAD")?;
    let commits = git(dir, &["rev-list", "--reverse", &format!("{base}..HEAD")])?;
    let mut approvals: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for sha in commits.lines() {
        let short = &sha[..sha.len().min(8)];
        let files = files_of(dir, sha)?;
        let signature_files: Vec<&String> = files
            .iter()
            .filter(|f| f.starts_with("signatures/"))
            .collect();
        let only_signatures = signature_files.len() == files.len();
        let approves = approved_kinds(dir, sha)?;
        if !only_signatures {
            if !signature_files.is_empty() {
                missing.push(Missing::Problem {
                    message: format!(
                        "Commit {short} changes signatures and other files: signatures go in \
                         commits of their own."
                    ),
                });
            }
            if !approves.is_empty() {
                missing.push(Missing::Problem {
                    message: format!(
                        "Commit {short} approves a change and makes one: approvals go in \
                         commits that change nothing but signatures."
                    ),
                });
            }
            continue;
        }
        if signature_files.is_empty() && approves.is_empty() {
            continue;
        }
        let Some(member) = verifier.signer(dir, sha) else {
            missing.push(Missing::Problem {
                message: format!(
                    "Commit {short} signs or approves, but is not signed with the key of a \
                     member listed in stemma.toml."
                ),
            });
            continue;
        };
        if !signature_files.is_empty() && !base_config.has_role(&member, &["signer".into()]) {
            missing.push(Missing::Problem {
                message: format!(
                    "Commit {short} changes signatures, but {member} is not a signer."
                ),
            });
        }
        for file in signature_files {
            if let Ok(text) = git(dir, &["show", &format!("{sha}:{file}")])
                && let Ok(s) = toml::from_str::<SignatureSigner>(&text)
                && s.signer != member
            {
                missing.push(Missing::Problem {
                    message: format!(
                        "{file} names {} as its signer, but {member} signed the commit.",
                        s.signer
                    ),
                });
            }
        }
        if !approves.is_empty() {
            let approved_content = trailer(dir, sha, "Approve-content")?;
            if approved_content != current {
                notes.push(format!(
                    "The approval of {member} in commit {short} is for other content: the \
                     change has changed since."
                ));
                continue;
            }
            for kind in approves {
                approvals.entry(kind).or_default().insert(member.clone());
            }
        }
    }

    for kind in kinds(dir, base)? {
        let Some(roles) = base_config.review_roles(&kind) else {
            continue;
        };
        let approved = approvals
            .get(&kind)
            .is_some_and(|members| members.iter().any(|m| base_config.has_role(m, &roles)));
        if !approved {
            missing.push(Missing::Approval {
                change: kind,
                roles,
            });
        }
    }
    for (name, member) in &base_config.members {
        if !member.roles.is_empty() && member.keys.is_empty() {
            notes.push(format!(
                "{name} has roles but no keys in stemma.toml, so cannot sign or approve."
            ));
        }
    }
    Ok(Verdict {
        base_config,
        missing,
        notes,
        approvals,
    })
}

/// The labels of the signature files at HEAD.
fn signature_labels(dir: &Path) -> Result<Vec<String>> {
    let listing = git(dir, &["ls-tree", "--name-only", "HEAD", "signatures/"])?;
    Ok(listing
        .lines()
        .filter_map(|path| path.strip_prefix("signatures/")?.strip_suffix(".toml"))
        .map(str::to_string)
        .collect())
}

/// `stemma verify`: fails when the change may not be merged.
pub fn verify(base: Option<String>, build: bool, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let base = base.unwrap_or_else(|| "origin/main".into());
    let mut problems = Vec::new();
    let report = if build {
        let checks = commands::run_checks(&library, json_output)?;
        if !checks.ok() {
            problems.push("`stemma check` fails: run it to see why.".to_string());
        }
        checks.report
    } else {
        None
    };
    let verdict = evaluate(&library, &base, report.as_ref())?;
    let mut failures: Vec<String> = problems;
    failures.extend(verdict.missing.iter().map(Missing::describe));
    let ok = failures.is_empty();
    if json_output {
        let value = json!({
            "ok": ok, "base": base, "failures": failures, "missing": verdict.missing,
            "notes": verdict.notes, "approvals": verdict.approvals,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        for f in &failures {
            ui::error(f);
        }
        for n in &verdict.notes {
            ui::note(n);
        }
        if ok {
            ui::success("The change carries every signature and approval it needs.");
        } else {
            ui::note("A signer or maintainer runs `stemma sign` in their own terminal.");
        }
    }
    Ok(ok)
}
