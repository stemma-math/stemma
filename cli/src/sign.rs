//! `stemma sign`: a person signs central environments, in their own terminal.

use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::agent::SESSION_VARIABLE;
use crate::library::Library;
use crate::report::{Environment, Signature};
use crate::scope::{Author, Scope};
use crate::verify::Missing;

/// The parts of an existing signature file that explain why it is stale.
#[derive(Deserialize)]
struct SignedState {
    fingerprints: crate::report::Fingerprints,
    #[serde(default)]
    closure: BTreeMap<String, String>,
}

/// Why an environment awaits a signature.
fn reason(library: &Path, env: &Environment) -> String {
    let Some(label) = &env.record.label else {
        return "new".into();
    };
    let path = library.join("signatures").join(format!("{label}.toml"));
    let Some(old) = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| toml::from_str::<SignedState>(&t).ok())
    else {
        return "new".into();
    };
    let Some(current) = &env.fingerprints else {
        return "not formalized".into();
    };
    let mut parts = Vec::new();
    if old.fingerprints.prose != current.prose {
        parts.push("its prose changed".to_string());
    }
    if old.fingerprints.formal != current.formal {
        // Name what changed, not what it now uses or no longer uses: those
        // follow from the change, and naming them would split one cause in many.
        let changed: BTreeSet<&str> = env
            .closure
            .iter()
            .filter(|(n, h)| old.closure.get(n).is_some_and(|old| old != h))
            .map(|(n, _)| n.as_str())
            .collect();
        if changed.is_empty() {
            parts.push("something its dependencies use changed".into());
        } else {
            let list: Vec<_> = changed.into_iter().collect();
            parts.push(format!("{} changed", list.join(", ")));
        }
    }
    if parts.is_empty() {
        "its marks changed".into()
    } else {
        parts.join("; ")
    }
}

/// The current time, as an RFC 3339 timestamp in UTC.
fn now_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// A TOML string literal.
fn quoted(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// The text of a signature file.
fn signature_file(env: &Environment, signer: &str, signed: &str, by: Option<&str>) -> String {
    let r = &env.record;
    let fp = env
        .fingerprints
        .as_ref()
        .expect("only formalized environments are signed");
    let mut out = format!(
        "label = {}\nkind = {}\ncentral = {}\n",
        quoted(r.label.as_deref().unwrap_or_default()),
        quoted(&r.base),
        r.central
    );
    if let Some(cited) = &r.cited {
        out.push_str(&format!("cited = {}\n", quoted(cited)));
    }
    out.push_str(&format!(
        "\nsigner = {}\nsigned = {signed}\n",
        quoted(signer)
    ));
    if let Some(by) = by {
        out.push_str(&format!("agent = {}\n", quoted(by)));
    }
    out.push_str(&format!(
        "\n[fingerprints]\nversion = {}\nprose = {}\nformal = {}\n\n[closure]\n",
        fp.version,
        quoted(&fp.prose),
        quoted(&fp.formal)
    ));
    for (name, hash) in &env.closure {
        out.push_str(&format!("{} = {}\n", quoted(name), quoted(hash)));
    }
    out
}

/// The agent named by the last commit that touched a module, if any.
fn proposing_agent(library: &Library, module: &str) -> Option<String> {
    let path = library.module_path(module);
    let out = Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%(trailers:key=Agent,valueonly)",
            "--",
        ])
        .arg(path)
        .current_dir(&library.dir)
        .output()
        .ok()?;
    let agent = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!agent.is_empty()).then_some(agent)
}

/// The read-back of an environment, when a current one exists: a link to the
/// page that shows read-backs beside their prose.
pub fn readback_link(library: &Library, env: &Environment) -> Option<String> {
    #[derive(Deserialize)]
    struct Made {
        formal: String,
    }
    let label = env.record.label.as_deref()?;
    let dir = library.dir.join(".stemma").join("readbacks");
    let text = std::fs::read_to_string(dir.join(format!("{label}.json"))).ok()?;
    let made: Made = serde_json::from_str(&text).ok()?;
    let current = env.fingerprints.as_ref()?.formal == made.formal;
    let page = dir.join("index.html");
    (current && page.is_file()).then(|| format!("file://{}", page.display()))
}

/// A central environment awaiting the signer.
struct Pending<'a> {
    env: &'a Environment,
    label: String,
    /// Why: new, or what changed since its signature.
    reason: String,
    /// Whether this branch touches it (see `scope`).
    mine: bool,
    /// Who last touched it.
    author: Option<Author>,
}

impl Pending<'_> {
    fn is_new(&self) -> bool {
        self.reason == "new"
    }

    fn card(&self, library: &Library, label_width: usize) -> String {
        let r = &self.env.record;
        let path = library.module_path(&r.module);
        let file = path.strip_prefix(&library.dir).unwrap_or(&path);
        let (start, end) = r.lines();
        crate::card::Card {
            display: &r.display,
            label: &self.label,
            stale: (!self.is_new()).then_some(self.reason.as_str()),
            prose: &r.prose,
            lean: &r.lean,
            base: &r.base,
            location: format!("{}:{start}-{end}", file.display()),
            author: if self.mine {
                None
            } else {
                Some(
                    self.author
                        .as_ref()
                        .map_or_else(|| "nobody yet (uncommitted)".into(), Author::describe),
                )
            },
            readback: readback_link(library, self.env),
        }
        .render(label_width)
    }
}

/// Asks which of `items` to sign, one by one; `preselect` checks them all
/// first.
fn choose<'a, 'b>(
    prompt: &str,
    items: &[&'b Pending<'a>],
    preselect: bool,
    label_width: usize,
) -> Result<Vec<&'b Pending<'a>>> {
    let mut select = cliclack::multiselect(prompt).required(false);
    for (i, p) in items.iter().enumerate() {
        let state = if p.is_new() { "new" } else { "stale" };
        let label = format!(
            "{:<label_width$}  {}",
            p.label,
            crate::card::dim(format!("{} · {state}", p.env.record.display))
        );
        select = select.item(i, label, "");
    }
    if preselect {
        select = select.initial_values((0..items.len()).collect());
    }
    let picked = select.interact()?;
    Ok(picked.into_iter().map(|i| items[i]).collect())
}

/// Offers to register a signing key that belongs to no member, then stops:
/// registering a key and signing are separate acts.
fn offer_registration(library: &Library, key: &str) -> Result<bool> {
    cliclack::log::warning(format!(
        "Your signing key belongs to no member of stemma.toml:\n{}",
        crate::card::dim(key)
    ))?;
    if library.config.members.is_empty() {
        cliclack::outro_cancel("stemma.toml has no members: a maintainer adds you first.")?;
        return Ok(false);
    }
    let me = crate::agent::person();
    let mut select = cliclack::select("Register it as the key of which member?");
    for (name, m) in &library.config.members {
        let hint = if m.keys.is_empty() { "no key yet" } else { "" };
        select = select.item(name.clone(), name, hint);
    }
    select = select.item(String::new(), "Nobody: do not register it", "");
    if library.config.members.contains_key(&me) {
        select = select.initial_value(me);
    }
    let member = select.interact()?;
    if member.is_empty() {
        cliclack::outro("Nothing registered.")?;
        return Ok(true);
    }
    let confirmed = cliclack::confirm(format!(
        "Add this key to {member} in stemma.toml, in a change a maintainer approves?"
    ))
    .initial_value(false)
    .interact()?;
    if !confirmed {
        cliclack::outro("Nothing registered.")?;
        return Ok(true);
    }
    let r = crate::keys::register(library, key, &member, true)?;
    cliclack::log::success(format!(
        "Added the key to {} in stemma.toml, committed on {}.",
        r.member, r.branch
    ))?;
    for line in crate::keys::after_registration(&r) {
        cliclack::log::info(crate::card::fill(&line))?;
    }
    cliclack::outro("Registered, not signed: run `stemma sign` again once the change is merged.")?;
    Ok(true)
}

/// `stemma sign`: shows what awaits the person's act, and records what they
/// choose: signatures of central environments, one by one, and approvals of
/// the change on this branch, in one commit signed with their key.
pub fn sign(labels: Vec<String>, base: String, commit: bool) -> Result<bool> {
    if std::env::var_os(SESSION_VARIABLE).is_some() {
        bail!(
            "signing is a person's act: run `stemma sign` in your own terminal, not in an agent's session"
        );
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("`stemma sign` runs only in an interactive terminal");
    }
    let library = Library::find(Path::new("."))?;
    let Some(key) = crate::keys::signing_key(&library.dir) else {
        bail!(
            "you sign with your key, and git has none configured. With an SSH key, for \
             example:\n  git config --global gpg.format ssh\n  \
             git config --global user.signingkey ~/.ssh/id_ed25519.pub"
        );
    };
    cliclack::intro(crate::card::bold(" stemma sign "))?;
    if library.config.member_with_key(&key).is_none() {
        return offer_registration(&library, &key);
    }
    let spinner = cliclack::spinner();
    spinner.start("Building the library and reading its state");
    let report = crate::commands::build_and_extract(&library, true);
    spinner.clear();
    let report = match report? {
        Ok(report) => report,
        Err(output) => {
            cliclack::outro_cancel(format!("The library does not build:\n{output}"))?;
            return Ok(false);
        }
    };
    let verdict = crate::verify::evaluate(&library, &base, Some(&report))?;
    // What counts is what the base's members sign: a key only registered on
    // this branch signs nothing yet.
    let Some(member) = verdict
        .base_config
        .member_with_key(&key)
        .map(str::to_string)
    else {
        cliclack::outro_cancel(format!(
            "Your key is in stemma.toml on this branch, but not on {base}: nothing you sign or \
             approve counts until a maintainer whose key is there approves the change and it \
             is merged."
        ))?;
        return Ok(false);
    };
    let is_signer = verdict.base_config.has_role(&member, &["signer".into()]);
    let scope = Scope::of_branch(&library.dir, &base)?;

    // Environments to sign: this branch's, and those others left pending.
    let mut pending = Vec::new();
    let mut unformalized = Vec::new();
    for env in &report.environments {
        let state = env.signature(&library.dir);
        if !is_signer || !matches!(state, Some(Signature::Unsigned | Signature::Stale)) {
            continue;
        }
        let label = env.record.label.clone().unwrap_or_default();
        if !labels.is_empty() && !labels.contains(&label) {
            continue;
        }
        if env.fingerprints.is_none() {
            unformalized.push(label);
            continue;
        }
        pending.push(Pending {
            env,
            reason: reason(&library.dir, env),
            mine: scope.contains(&library, env),
            author: Author::of(&library, env),
            label,
        });
    }
    // Signatures that apply to no central environment any more.
    let to_withdraw: Vec<String> = if is_signer {
        verdict
            .missing
            .iter()
            .filter_map(|m| match m {
                Missing::Withdrawal { label } => Some(label.clone()),
                _ => None,
            })
            .collect()
    } else {
        Vec::new()
    };
    // Changes to approve.
    let to_approve: Vec<String> = verdict
        .missing
        .iter()
        .filter(|m| m.can_supply(&verdict.base_config, &member))
        .filter_map(|m| match m {
            Missing::Approval { change, .. } => Some(change.clone()),
            _ => None,
        })
        .collect();

    if !is_signer {
        cliclack::log::info(format!(
            "{member} is not a signer on {base}: you can approve changes, not sign environments."
        ))?;
    }
    let unknown: Vec<&String> = labels
        .iter()
        .filter(|l| !pending.iter().any(|p| &p.label == *l) && !unformalized.contains(l))
        .collect();
    if is_signer && !unknown.is_empty() {
        cliclack::log::warning(format!(
            "Nothing of these awaits a signature: {}.",
            unknown
                .iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
    }
    if !unformalized.is_empty() {
        cliclack::log::warning(format!(
            "Not formalized yet, so they cannot be signed: {}.",
            unformalized.join(", ")
        ))?;
    }
    if pending.is_empty() && to_approve.is_empty() && to_withdraw.is_empty() {
        cliclack::outro(format!(
            "Nothing awaits your signature or approval, {member}."
        ))?;
        return Ok(true);
    }

    let label_width = pending
        .iter()
        .map(|p| p.label.chars().count())
        .max()
        .unwrap_or(0);
    let mine: Vec<&Pending> = pending.iter().filter(|p| p.mine).collect();
    let others: Vec<&Pending> = pending.iter().filter(|p| !p.mine).collect();
    let mut chosen: Vec<&Pending> = Vec::new();
    if !mine.is_empty() {
        cliclack::log::step(crate::card::bold(format!(
            "This branch: {} awaiting a signature",
            mine.len()
        )))?;
        for p in &mine {
            cliclack::log::remark(p.card(&library, label_width))?;
        }
        chosen.extend(choose(
            "Sign which? Only what you have read: the prose and the Lean must say the same \
             thing.",
            &mine,
            true,
            label_width,
        )?);
    }
    if !others.is_empty() {
        cliclack::log::step(crate::card::bold(format!(
            "Left pending by others: {}",
            others.len()
        )))?;
        for p in &others {
            cliclack::log::remark(p.card(&library, label_width))?;
        }
        chosen.extend(choose(
            "Sign any of these too? None is selected: choose only what you have checked.",
            &others,
            !labels.is_empty(),
            label_width,
        )?);
    }
    let mut withdrawn = Vec::new();
    if !to_withdraw.is_empty() {
        cliclack::log::step(crate::card::bold(format!(
            "Signatures that apply to no central environment: {}",
            to_withdraw.len()
        )))?;
        cliclack::log::remark(format!(
            "{}\n{}",
            to_withdraw.join(", "),
            crate::card::dim("Removed, made not central, or relabelled.")
        ))?;
        if cliclack::confirm(format!(
            "Withdraw these {}? The library no longer claims what they signed.",
            to_withdraw.len()
        ))
        .initial_value(false)
        .interact()?
        {
            withdrawn = to_withdraw;
        }
    }
    let mut approved = Vec::new();
    if !to_approve.is_empty() {
        cliclack::log::step(crate::card::bold(format!(
            "Approvals, apart from signatures: a change of kind {}",
            to_approve.join(", ")
        )))?;
        let dirty = Command::new("git")
            .args(["status", "--porcelain", "--", ".", ":!signatures"])
            .current_dir(&library.dir)
            .output()?;
        if !dirty.stdout.is_empty() {
            cliclack::log::warning(
                "There are uncommitted changes: they are not part of what you approve.",
            )?;
        }
        let stat = Command::new("git")
            .args(["diff", "--stat", &format!("{base}...HEAD")])
            .current_dir(&library.dir)
            .output()?;
        cliclack::log::remark(format!(
            "{}\n{}",
            crate::card::dim(format!("What this branch changes, against {base}:")),
            String::from_utf8_lossy(&stat.stdout).trim_end()
        ))?;
        if cliclack::confirm(format!(
            "Approve this change ({})? Only if you have looked at it.",
            to_approve.join(", ")
        ))
        .initial_value(false)
        .interact()?
        {
            approved = to_approve;
        }
    }

    let written: Vec<String> = chosen.iter().map(|p| p.label.clone()).collect();
    let still = still_missing(&verdict.missing, &pending, &written, &withdrawn, &approved);
    if written.is_empty() && approved.is_empty() && withdrawn.is_empty() {
        if !still.is_empty() {
            cliclack::log::warning(format!("Still missing:\n{}", still.join("\n")))?;
        }
        cliclack::outro("Nothing was signed or approved.")?;
        return Ok(true);
    }
    let signed = now_utc();
    if !written.is_empty() {
        let dir = library.dir.join("signatures");
        std::fs::create_dir_all(&dir)?;
        for p in &chosen {
            let by = p
                .author
                .as_ref()
                .and_then(|a| a.agent.clone())
                .or_else(|| proposing_agent(&library, &p.env.record.module));
            let path = dir.join(format!("{}.toml", p.label));
            std::fs::write(
                &path,
                signature_file(p.env, &member, &signed, by.as_deref()),
            )?;
        }
    }
    if !commit {
        if !approved.is_empty() {
            cliclack::log::warning(
                "Approvals are recorded in a commit: with --no-commit, none is.",
            )?;
        }
        cliclack::outro(format!("Signed (not committed): {}.", written.join(", ")))?;
        return Ok(true);
    }
    let files: Vec<String> = written
        .iter()
        .chain(&withdrawn)
        .map(|l| format!("signatures/{l}.toml"))
        .collect();
    for l in &withdrawn {
        std::fs::remove_file(library.dir.join("signatures").join(format!("{l}.toml")))?;
    }
    let mut subject = Vec::new();
    if !written.is_empty() {
        subject.push(format!("Sign {}", written.join(", ")));
    }
    if !withdrawn.is_empty() {
        subject.push(format!("withdraw {}", withdrawn.join(", ")));
    }
    if !approved.is_empty() {
        subject.push(format!("approve the change ({})", approved.join(", ")));
    }
    let mut summary = subject.join("; ");
    if let Some(first) = summary.get(..1) {
        summary = first.to_uppercase() + &summary[1..];
    }
    let mut message = summary.clone();
    if !approved.is_empty() {
        // The approval names the content it approves: the committed content of
        // the branch, which the signatures in this commit do not change.
        let content = crate::verify::content_id(&library.dir, "HEAD")?;
        message.push_str(&format!(
            "\n\nApprove: {}\nApprove-content: {content}\nApproved-by: {member}",
            approved.join(", ")
        ));
    }
    if !files.is_empty() {
        let added = Command::new("git")
            .args(["add", "--all", "--"])
            .args(&files)
            .current_dir(&library.dir)
            .status()?;
        anyhow::ensure!(added.success(), "could not stage the signatures");
    }
    let mut git_commit = Command::new("git");
    git_commit
        .args(["commit", "-S", "--quiet", "-m", &message])
        .current_dir(&library.dir);
    if files.is_empty() {
        git_commit.arg("--allow-empty");
    } else {
        git_commit.arg("--").args(&files);
    }
    let committed = git_commit.status().context("running `git commit`")?;
    anyhow::ensure!(committed.success(), "committing failed");
    if !still.is_empty() {
        cliclack::log::warning(format!("Still missing:\n{}", still.join("\n")))?;
    }
    cliclack::outro(format!("{summary}. Committed, signed with your key."))?;
    Ok(true)
}

/// What the change still lacks once the person has acted, one line each.
fn still_missing(
    missing: &[Missing],
    pending: &[Pending],
    written: &[String],
    withdrawn: &[String],
    approved: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    for m in missing {
        let supplied = match m {
            Missing::Signature { label, .. } => written.contains(label),
            Missing::Withdrawal { label } => withdrawn.contains(label),
            Missing::Approval { change, .. } => approved.contains(change),
            Missing::Problem { .. } => false,
        };
        if !supplied {
            out.push(format!("  {}", m.describe()));
        }
    }
    // Unsigned environments the policy does not require now.
    for p in pending {
        let listed = missing
            .iter()
            .any(|m| matches!(m, Missing::Signature { label, .. } if *label == p.label));
        if !listed && !written.contains(&p.label) {
            out.push(format!("  '{}' is central and unsigned.", p.label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_rfc_3339() {
        let t = now_utc();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
    }
}
