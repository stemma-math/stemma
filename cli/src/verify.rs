//! `stemma verify`: what the forge checks on a pull request, beyond
//! `stemma check`: signatures and the group's policy.
//!
//! The policy is always read from the base of the pull request, never from the
//! pull request itself, so that a pull request cannot change what judges it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

use crate::commands;
use crate::config::Config;
use crate::library::Library;
use crate::report::{Report, Signature};

/// The pull request being verified, from the forge's event.
struct PullRequest {
    number: u64,
    author: String,
    base: String,
}

/// Where `stemma verify` runs: a repository on GitHub, maybe for a pull request.
struct Forge {
    repository: String,
    pull_request: Option<PullRequest>,
}

/// The forge context of a GitHub Actions run, if this is one.
fn forge() -> Result<Option<Forge>> {
    let Ok(repository) = std::env::var("GITHUB_REPOSITORY") else {
        return Ok(None);
    };
    let pull_request = match std::env::var("GITHUB_EVENT_PATH") {
        Ok(path) => {
            let event: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            let pr = &event["pull_request"];
            pr["number"].as_u64().map(|number| PullRequest {
                number,
                author: pr["user"]["login"].as_str().unwrap_or_default().to_string(),
                base: pr["base"]["sha"].as_str().unwrap_or_default().to_string(),
            })
        }
        Err(_) => None,
    };
    Ok(Some(Forge {
        repository,
        pull_request,
    }))
}

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

fn gh(args: &[&str]) -> Result<String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .with_context(|| format!("running `gh {}`", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "`gh {}` failed:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What a signature file says about its signer.
#[derive(Deserialize)]
struct SignatureSigner {
    signer: String,
}

/// The kinds of change a pull request makes, for the policy's reviews.
fn kinds(dir: &Path, base: &str, report: Option<&Report>) -> Result<BTreeSet<String>> {
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
    for env in report
        .map(|r| r.environments.as_slice())
        .unwrap_or_default()
    {
        let (true, Some(label)) = (env.is_central_claim(), &env.record.label) else {
            continue;
        };
        let pattern = format!("label := \"{label}\"");
        let lines = Command::new("git")
            .args(["grep", "-h", "-F", "-e", &pattern, base, "--", "*.lean"])
            .current_dir(dir)
            .output()?;
        let lines = String::from_utf8_lossy(&lines.stdout);
        if !lines.lines().any(|l| l.contains("central := true")) {
            out.insert("new-central".to_string());
        }
    }
    Ok(out)
}

/// The accounts that approve the pull request in its latest review.
fn approvals(repository: &str, number: u64) -> Result<BTreeSet<String>> {
    let text = gh(&[
        "api",
        "--paginate",
        &format!("repos/{repository}/pulls/{number}/reviews"),
        "--jq",
        ".[] | [.user.login, .state] | @tsv",
    ])?;
    let mut latest: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        if let Some((user, state)) = line.split_once('\t')
            && state != "COMMENTED"
        {
            latest.insert(user.to_string(), state.to_string());
        }
    }
    Ok(latest
        .into_iter()
        .filter(|(_, s)| s == "APPROVED")
        .map(|(u, _)| u)
        .collect())
}

/// `stemma verify`: fails when the pull request may not be merged.
pub fn verify(base: Option<String>, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let dir = &library.dir;
    let forge = forge()?;
    let pr = forge.as_ref().and_then(|f| f.pull_request.as_ref());
    let base = base
        .or_else(|| pr.map(|p| p.base.clone()))
        .unwrap_or_else(|| "origin/main".into());
    let base_config = match git(dir, &["show", &format!("{base}:stemma.toml")]) {
        Ok(text) => Config::parse(&text).context("reading stemma.toml at the base")?,
        Err(_) => Config::read(dir)?,
    };
    let mut failures: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // The library itself.
    let checks = commands::run_checks(&library)?;
    if !checks.ok() {
        failures.push("`stemma check` fails: run it to see why.".into());
    }
    if let Some(report) = &checks.report {
        for env in &report.environments {
            if env.signature(dir) == Some(Signature::Stale) {
                failures.push(format!(
                    "The signature of '{}' is stale: it must be signed again.",
                    env.record.label.as_deref().unwrap_or_default()
                ));
            }
        }
    }

    // Signatures: commits that touch `signatures/` touch nothing else, and are
    // verified commits of a signer.
    let commits = git(dir, &["rev-list", &format!("{base}..HEAD")])?;
    for sha in commits.lines() {
        let files = git(
            dir,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", sha],
        )?;
        let files: Vec<&str> = files.lines().collect();
        let signature_files: Vec<&str> = files
            .iter()
            .copied()
            .filter(|f| f.starts_with("signatures/"))
            .collect();
        if signature_files.is_empty() {
            continue;
        }
        let short = &sha[..sha.len().min(8)];
        if signature_files.len() != files.len() {
            failures.push(format!(
                "Commit {short} changes signatures and other files: signatures go in commits of their own."
            ));
        }
        let Some(forge) = &forge else {
            notes.push(format!(
                "Commit {short} changes signatures; who signed it is verified only on the forge."
            ));
            continue;
        };
        let info = gh(&[
            "api",
            &format!("repos/{}/commits/{sha}", forge.repository),
            "--jq",
            "[.commit.verification.verified, (.author.login // \"\")] | @tsv",
        ])?;
        let (verified, login) = info.split_once('\t').unwrap_or(("false", ""));
        if verified != "true" {
            failures.push(format!(
                "Commit {short} changes signatures but is not a verified signed commit."
            ));
            continue;
        }
        if !base_config.has_role(login, &["signer".to_string()]) {
            failures.push(format!(
                "Commit {short} changes signatures, but its author, {login}, is not a signer."
            ));
        }
        for file in signature_files {
            if let Ok(text) = git(dir, &["show", &format!("{sha}:{file}")])
                && let Ok(s) = toml::from_str::<SignatureSigner>(&text)
                && s.signer != login
            {
                failures.push(format!(
                    "{file} names {} as its signer, but {login} committed it.",
                    s.signer
                ));
            }
        }
        if let Some(pr) = pr
            && base_config.signatures.distinct_from_author
            && pr.author == login
        {
            failures.push(format!(
                "{login} signed in their own pull request, and this library asks for a different signer."
            ));
        }
    }

    // The policy, for a pull request.
    let mut kinds_found = BTreeSet::new();
    if let (Some(forge), Some(pr)) = (&forge, pr) {
        kinds_found = kinds(dir, &base, checks.report.as_ref())?;
        let approved: BTreeSet<String> = approvals(&forge.repository, pr.number)?
            .into_iter()
            .filter(|u| *u != pr.author)
            .collect();
        let approved_by = |roles: Option<&[String]>| {
            approved.iter().any(|u| match roles {
                Some(roles) => base_config.has_role(u, roles),
                None => base_config.members.contains_key(u),
            })
        };
        if !base_config.may_self_merge(&pr.author)
            && !approved_by(base_config.policy.self_merge.as_deref())
        {
            failures.push(format!(
                "{} may not merge their own pull requests: it needs the approval of a member who may.",
                pr.author
            ));
        }
        for kind in &kinds_found {
            if let Some(roles) = base_config.review_roles(kind)
                && !approved_by(Some(&roles))
            {
                failures.push(format!(
                    "A change of kind '{kind}' needs the approval of a member with one of these roles: {}.",
                    roles.join(", ")
                ));
            }
        }
    } else {
        notes.push("The policy is checked only for pull requests on the forge.".into());
    }

    let ok = failures.is_empty();
    if json_output {
        let value = json!({
            "ok": ok, "base": base, "failures": failures, "notes": notes,
            "kinds": kinds_found,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        for f in &failures {
            println!("error: {f}");
        }
        for n in &notes {
            println!("note: {n}");
        }
        if ok {
            println!("The pull request may be merged.");
        }
    }
    Ok(ok)
}
