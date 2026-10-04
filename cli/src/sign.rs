//! `stemma sign`: a person signs central environments, in their own terminal.

use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::agent::{self, SESSION_VARIABLE};
use crate::library::Library;
use crate::report::{Environment, Signature};

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

/// `stemma sign`: shows what awaits a signature, grouped by cause, and signs
/// what the person confirms.
pub fn sign(labels: Vec<String>, commit: bool) -> Result<bool> {
    if std::env::var_os(SESSION_VARIABLE).is_some() {
        bail!(
            "signing is a person's act: run `stemma sign` in your own terminal, not in an agent's session"
        );
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("`stemma sign` runs only in an interactive terminal");
    }
    let library = Library::find(Path::new("."))?;
    if commit {
        let key = Command::new("git")
            .args(["config", "user.signingkey"])
            .current_dir(&library.dir)
            .output()?;
        if String::from_utf8_lossy(&key.stdout).trim().is_empty() {
            bail!(
                "signatures are committed signed with your key, and git has none configured. \
                 With an SSH key, for example:\n  git config --global gpg.format ssh\n  \
                 git config --global user.signingkey ~/.ssh/id_ed25519.pub"
            );
        }
    }
    cliclack::intro(crate::ui::bold(" stemma sign "))?;
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
    let mut groups: BTreeMap<String, Vec<&Environment>> = BTreeMap::new();
    let mut unformalized = Vec::new();
    for env in &report.environments {
        let state = env.signature(&library.dir);
        if !matches!(state, Some(Signature::Unsigned | Signature::Stale)) {
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
        groups
            .entry(reason(&library.dir, env))
            .or_default()
            .push(env);
    }
    if !unformalized.is_empty() {
        cliclack::log::warning(format!(
            "Not formalized yet, so they cannot be signed: {}.",
            unformalized.join(", ")
        ))?;
    }
    if groups.is_empty() {
        cliclack::outro("Nothing awaits a signature.")?;
        return Ok(true);
    }
    let signer = agent::person();
    let signed = now_utc();
    let mut written = Vec::new();
    for (reason, envs) in &groups {
        let heading = if reason == "new" {
            format!("New central environments ({})", envs.len())
        } else {
            format!("Stale: {reason} ({})", envs.len())
        };
        cliclack::log::step(crate::ui::bold(heading))?;
        for env in envs {
            let r = &env.record;
            cliclack::note(
                format!(
                    "{} {}  {}",
                    r.display,
                    r.label.as_deref().unwrap_or_default(),
                    crate::ui::dim(format!("{}, line {}", r.module, r.line))
                ),
                format!("{}\n\n{}", r.prose, r.lean),
            )?;
        }
        let sign = cliclack::confirm(format!(
            "Sign these {}? Only if the prose and the Lean say the same thing.",
            envs.len()
        ))
        .initial_value(false)
        .interact()?;
        if !sign {
            continue;
        }
        let dir = library.dir.join("signatures");
        std::fs::create_dir_all(&dir)?;
        for env in envs {
            let label = env.record.label.as_deref().unwrap_or_default();
            let by = proposing_agent(&library, &env.record.module);
            let path = dir.join(format!("{label}.toml"));
            std::fs::write(&path, signature_file(env, &signer, &signed, by.as_deref()))?;
            written.push(label.to_string());
        }
    }
    if written.is_empty() {
        cliclack::outro("Nothing was signed.")?;
        return Ok(true);
    }
    if commit {
        let files: Vec<String> = written
            .iter()
            .map(|l| format!("signatures/{l}.toml"))
            .collect();
        let added = Command::new("git")
            .arg("add")
            .args(&files)
            .current_dir(&library.dir)
            .status()?;
        anyhow::ensure!(added.success(), "could not stage the signatures");
        let message = format!("Sign {}", written.join(", "));
        let committed = Command::new("git")
            .args(["commit", "-S", "--quiet", "-m", &message, "--"])
            .args(&files)
            .current_dir(&library.dir)
            .status()
            .context("running `git commit`")?;
        anyhow::ensure!(
            committed.success(),
            "the signature files are written, but committing them failed"
        );
        cliclack::outro(format!("Signed and committed: {}.", written.join(", ")))?;
    } else {
        cliclack::outro(format!("Signed (not committed): {}.", written.join(", ")))?;
    }
    Ok(true)
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
