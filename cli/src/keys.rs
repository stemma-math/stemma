//! Keys: who someone is, for Stemma, is the key they sign with.
//!
//! Members list their SSH public keys in `stemma.toml`. A signed commit's
//! author is the member whose key made the signature, checked by git itself,
//! with no forge involved.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use toml_edit::{Array, DocumentMut, Item, value};

use crate::config::Config;
use crate::library::Library;

/// A key without its comment: its type and its data.
pub fn normalize(key: &str) -> String {
    key.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

/// The public key git signs with here (`user.signingkey`), if there is one.
pub fn signing_key(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["config", "user.signingkey"])
        .current_dir(dir)
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if value.is_empty() {
        return None;
    }
    if let Some(literal) = value.strip_prefix("key::") {
        return Some(normalize(literal));
    }
    if is_public_key(&value) {
        return Some(normalize(&value));
    }
    let path = match value.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from)?.join(rest),
        None => PathBuf::from(&value),
    };
    read_public_key(&path)
}

/// Whether a text looks like an SSH public key: a key type, then its data.
fn is_public_key(text: &str) -> bool {
    let mut parts = text.split_whitespace();
    let kind = parts.next().unwrap_or_default();
    let known = kind.starts_with("ssh-")
        || kind.starts_with("ecdsa-")
        || kind.starts_with("sk-ssh-")
        || kind.starts_with("sk-ecdsa-");
    known && parts.next().is_some()
}

/// The public key in a file: the file itself when it is one, or the `.pub`
/// beside a private key.
pub fn read_public_key(path: &Path) -> Option<String> {
    let public = if path.extension().is_some_and(|e| e == "pub") {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("{}.pub", path.display()))
    };
    [public.as_path(), path].into_iter().find_map(|p| {
        std::fs::read_to_string(p)
            .ok()
            .filter(|k| is_public_key(k))
            .map(|k| normalize(&k))
    })
}

/// Adds `key` to the keys of `member` in the text of a `stemma.toml`. Fails
/// when there is no such member.
pub fn add_key(doc: &mut DocumentMut, member: &str, key: &str) -> Result<()> {
    let entry = doc
        .get_mut("members")
        .and_then(Item::as_table_like_mut)
        .and_then(|m| m.get_mut(member))
        .and_then(Item::as_table_like_mut)
        .with_context(|| format!("stemma.toml has no member '{member}'"))?;
    match entry.get_mut("keys").and_then(Item::as_array_mut) {
        Some(keys) => {
            if !keys
                .iter()
                .any(|k| k.as_str().is_some_and(|k| normalize(k) == normalize(key)))
            {
                keys.push(key);
            }
        }
        None => {
            entry.insert("keys", value(Array::from_iter([key])));
        }
    }
    Ok(())
}

/// A key added to a member, on a branch.
pub struct Registration {
    pub member: String,
    pub key: String,
    pub branch: String,
    /// Whether the work was on `main`, and moved to `branch`.
    pub moved_from_main: bool,
    pub committed: bool,
}

/// Adds `key` to `member`'s entry of `stemma.toml`, on a branch, never on
/// `main`, and commits it unless `commit` is false. It is a change of the
/// policy: a maintainer whose key is already on `main` approves it, so nobody
/// approves their own first key.
pub fn register(library: &Library, key: &str, member: &str, commit: bool) -> Result<Registration> {
    let dir = &library.dir;
    let key = normalize(key);
    ensure!(is_public_key(&key), "'{key}' is not an SSH public key");
    if let Some(owner) = library.config.member_with_key(&key) {
        bail!("this key is already {owner}'s in stemma.toml");
    }
    if !library.config.members.contains_key(member) {
        let names: Vec<&str> = library.config.members.keys().map(String::as_str).collect();
        bail!(
            "stemma.toml has no member '{member}' (its members: {}); adding a member is \
             another change of the policy",
            names.join(", ")
        );
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--", "stemma.toml"])
        .current_dir(dir)
        .output()
        .context("running `git status`")?;
    ensure!(
        dirty.status.success() && dirty.stdout.is_empty(),
        "stemma.toml has uncommitted changes: commit or discard them first"
    );
    let Some(mut branch) = crate::branches::current(dir) else {
        bail!("HEAD is on no branch: switch to one first");
    };
    let moved_from_main = branch == "main";
    if moved_from_main {
        branch = crate::branches::move_to_new(dir, &format!("keys/{member}"))?;
    }
    let path = dir.join("stemma.toml");
    let mut doc = std::fs::read_to_string(&path)?
        .parse::<DocumentMut>()
        .context("reading stemma.toml")?;
    add_key(&mut doc, member, &key)?;
    std::fs::write(&path, doc.to_string())?;
    if commit {
        let message = format!("Add {member}'s signing key");
        let out = Command::new("git")
            .args(["commit", "--quiet", "-m", &message, "--", "stemma.toml"])
            .current_dir(dir)
            .output()
            .context("running `git commit`")?;
        ensure!(
            out.status.success(),
            "committing stemma.toml failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(Registration {
        member: member.to_string(),
        key,
        branch,
        moved_from_main,
        committed: commit,
    })
}

/// What a person should know after registering a key, one line each.
pub fn after_registration(r: &Registration) -> Vec<String> {
    let mut out = Vec::new();
    if r.moved_from_main {
        out.push(format!(
            "You were on `main`: the change is on the branch {}.",
            r.branch
        ));
    }
    out.push(
        "A maintainer whose key is already on `main` approves this change with `stemma sign`: \
         nobody approves their own first key. Share it with `stemma share`."
            .into(),
    );
    out.push(
        "Until it is merged, nothing you sign or approve counts. Then run `stemma sign`.".into(),
    );
    out.push(
        "Prefer a key dedicated to signing, not the one you log in with: agents run with your \
         permissions."
            .into(),
    );
    out
}

/// `stemma key add`: registers the key the person signs with.
pub fn key_add(
    key_path: Option<PathBuf>,
    member: Option<String>,
    commit: bool,
    json_output: bool,
) -> Result<bool> {
    if std::env::var_os(crate::agent::SESSION_VARIABLE).is_some() {
        bail!(
            "registering a key is a person's act: run `stemma key add` in your own terminal, \
             not in an agent's session"
        );
    }
    let library = Library::find(Path::new("."))?;
    let key = match &key_path {
        Some(path) => read_public_key(path)
            .with_context(|| format!("{} holds no SSH public key", path.display()))?,
        None => signing_key(&library.dir).context(
            "git has no signing key configured (`user.signingkey`): name one with --key, or \
             configure it:\n  git config --global gpg.format ssh\n  \
             git config --global user.signingkey ~/.ssh/id_ed25519_signing.pub",
        )?,
    };
    let member = member.unwrap_or_else(crate::agent::person);
    let r = register(&library, &key, &member, commit)?;
    if json_output {
        let value = serde_json::json!({
            "member": r.member, "key": r.key, "branch": r.branch,
            "moved_from_main": r.moved_from_main, "committed": r.committed,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(true);
    }
    crate::ui::success(format!(
        "Added the key {} to {} in stemma.toml{}.",
        crate::ui::dim(&r.key),
        crate::ui::bold(&r.member),
        if r.committed {
            format!(", committed on {}", r.branch)
        } else {
            String::new()
        }
    ));
    for line in after_registration(&r) {
        crate::ui::note(line);
    }
    Ok(true)
}

/// The members' keys, as a git "allowed signers" file.
pub fn allowed_signers(config: &Config) -> String {
    let mut out = String::new();
    for (name, member) in &config.members {
        for key in &member.keys {
            out.push_str(&format!("{name} namespaces=\"git\" {}\n", normalize(key)));
        }
    }
    out
}

/// Checks signed commits against the members' keys.
pub struct Verifier {
    file: tempfile_path::TempFile,
}

mod tempfile_path {
    use std::path::PathBuf;

    /// A file removed when dropped.
    pub struct TempFile(pub PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

impl Verifier {
    /// A verifier for the members of `config`.
    pub fn new(config: &Config) -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "stemma-allowed-signers-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, allowed_signers(config))?;
        Ok(Self {
            file: tempfile_path::TempFile(path),
        })
    }

    /// The member who signed a commit, when its signature is good and made by a
    /// member's key.
    pub fn signer(&self, dir: &Path, commit: &str) -> Option<String> {
        let out = Command::new("git")
            .arg("-c")
            .arg(format!(
                "gpg.ssh.allowedSignersFile={}",
                self.file.0.display()
            ))
            .args(["log", "-1", "--format=%G?%x09%GS", commit])
            .current_dir(dir)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let (status, signer) = text.trim().split_once('\t')?;
        (status == "G" && !signer.is_empty()).then(|| signer.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_compare_without_comments() {
        assert_eq!(normalize("ssh-ed25519 AAAA me@host\n"), "ssh-ed25519 AAAA");
    }

    #[test]
    fn adds_keys_to_members_of_either_form() {
        let mut doc: DocumentMut = "[members]\nalice = { roles = [\"signer\"], keys = [] }\n\n\
            [members.bob]\nroles = [\"signer\"]\n"
            .parse()
            .unwrap();
        add_key(&mut doc, "alice", "ssh-ed25519 AAAA").unwrap();
        add_key(&mut doc, "alice", "ssh-ed25519 AAAA").unwrap();
        add_key(&mut doc, "bob", "ssh-ed25519 BBBB").unwrap();
        assert!(add_key(&mut doc, "carol", "ssh-ed25519 CCCC").is_err());
        let c = Config::parse(&format!(
            "[library]\nname = \"A\"\ntitle = \"A\"\nstemma = \"0.1.0\"\n\n{doc}"
        ))
        .unwrap();
        assert_eq!(c.members["alice"].keys, ["ssh-ed25519 AAAA"]);
        assert_eq!(c.member_with_key("ssh-ed25519 BBBB"), Some("bob"));
    }

    #[test]
    fn tells_public_keys_from_other_text() {
        assert!(is_public_key("ssh-ed25519 AAAA comment"));
        assert!(!is_public_key("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(!is_public_key("ssh-ed25519"));
    }
}
