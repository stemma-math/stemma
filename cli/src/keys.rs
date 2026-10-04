//! Keys: who someone is, for Stemma, is the key they sign with.
//!
//! Members list their SSH public keys in `stemma.toml`. A signed commit's
//! author is the member whose key made the signature, checked by git itself,
//! with no forge involved.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

use crate::config::Config;

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
    if value.starts_with("ssh-") || value.starts_with("ecdsa-") {
        return Some(normalize(&value));
    }
    let path = match value.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from)?.join(rest),
        None => PathBuf::from(&value),
    };
    let public = if path.extension().is_some_and(|e| e == "pub") {
        path
    } else {
        PathBuf::from(format!("{}.pub", path.display()))
    };
    std::fs::read_to_string(public).ok().map(|k| normalize(&k))
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
}
