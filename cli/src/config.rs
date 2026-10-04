//! The library's configuration, `stemma.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// The contents of `stemma.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub library: Library,
    /// The group's members, by forge account.
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
    #[serde(default)]
    pub policy: Policy,
}

/// The `[library]` table.
#[derive(Debug, Deserialize)]
pub struct Library {
    /// The library's Lean name, such as `Algebra`.
    pub name: String,
    /// The site's title.
    pub title: String,
    /// The `stemma` version the library uses.
    pub stemma: String,
}

/// A member of the group.
#[derive(Debug, Default, Deserialize)]
pub struct Member {
    #[serde(default)]
    pub roles: Vec<String>,
    /// The public keys the member signs with, as SSH public keys.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Which changes need whose approval.
#[derive(Debug, Default, Deserialize)]
pub struct Policy {
    /// Kinds of change, and the roles whose approval they need.
    #[serde(default)]
    pub review: BTreeMap<String, Vec<String>>,
}

impl Config {
    /// Reads `stemma.toml` from a library's directory.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join("stemma.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Parses the text of a `stemma.toml`.
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// The member who signs with `key`, if any.
    pub fn member_with_key(&self, key: &str) -> Option<&str> {
        let key = crate::keys::normalize(key);
        self.members
            .iter()
            .find(|(_, m)| m.keys.iter().any(|k| crate::keys::normalize(k) == key))
            .map(|(name, _)| name.as_str())
    }

    /// Whether `account` is a member with one of `roles`.
    pub fn has_role(&self, account: &str, roles: &[String]) -> bool {
        self.members
            .get(account)
            .is_some_and(|m| m.roles.iter().any(|r| roles.contains(r)))
    }

    /// The roles whose approval a kind of change needs. A change to the
    /// policy always needs a maintainer's; a change of dependencies needs one
    /// too unless the group says otherwise (`dependencies = []`).
    pub fn review_roles(&self, kind: &str) -> Option<Vec<String>> {
        let configured = self.policy.review.get(kind).cloned();
        if kind == "dependencies" {
            let roles = configured.unwrap_or_else(|| vec!["maintainer".into()]);
            return (!roles.is_empty()).then_some(roles);
        }
        if kind == "policy" {
            let mut roles = configured.unwrap_or_default();
            if !roles.iter().any(|r| r == "maintainer") {
                roles.push("maintainer".into());
            }
            return Some(roles);
        }
        configured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
[library]
name = "Alg"
title = "Alg"
stemma = "0.3.0"

[members]
alice = { roles = ["maintainer", "signer"], keys = ["ssh-ed25519 AAAAalice alice@laptop"] }
bob = { roles = ["signer"] }

[policy.review]
policy = ["signer"]
"#;

    #[test]
    fn reads_members_and_policy() {
        let c = Config::parse(CONFIG).unwrap();
        assert!(c.has_role("alice", &["maintainer".into()]));
        assert!(!c.has_role("bob", &["maintainer".into()]));
        assert_eq!(c.review_roles("policy").unwrap(), ["signer", "maintainer"]);
        assert_eq!(c.review_roles("dependencies").unwrap(), ["maintainer"]);
        let opted_out = Config::parse(&format!("{CONFIG}dependencies = []\n")).unwrap();
        assert!(opted_out.review_roles("dependencies").is_none());
    }

    #[test]
    fn finds_a_member_by_key_whatever_the_comment() {
        let c = Config::parse(CONFIG).unwrap();
        assert_eq!(
            c.member_with_key("ssh-ed25519 AAAAalice other"),
            Some("alice")
        );
        assert_eq!(c.member_with_key("ssh-ed25519 AAAAbob"), None);
    }
}
