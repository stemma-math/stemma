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
    #[serde(default)]
    pub signatures: Signatures,
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
}

/// Who may merge which pull requests.
#[derive(Debug, Default, Deserialize)]
pub struct Policy {
    /// The roles whose members may merge their pull requests without another
    /// member's approval, once every check passes. When absent, every member
    /// may. Merging is always done by a person.
    pub merge_without_approval: Option<Vec<String>>,
    /// Kinds of change, and the roles whose approval they need.
    #[serde(default)]
    pub review: BTreeMap<String, Vec<String>>,
}

/// The `[signatures]` table.
#[derive(Debug, Default, Deserialize)]
pub struct Signatures {
    /// Whether the signer must differ from the pull request's author.
    #[serde(default)]
    pub distinct_from_author: bool,
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

    /// Whether `account` is a member with one of `roles`.
    pub fn has_role(&self, account: &str, roles: &[String]) -> bool {
        self.members
            .get(account)
            .is_some_and(|m| m.roles.iter().any(|r| roles.contains(r)))
    }

    /// Whether `account` may merge their pull requests without another member's
    /// approval.
    pub fn may_merge_without_approval(&self, account: &str) -> bool {
        match &self.policy.merge_without_approval {
            None => self.members.contains_key(account),
            Some(roles) => self.has_role(account, roles),
        }
    }

    /// The roles whose approval a kind of change needs. A change to the
    /// policy always needs a maintainer's.
    pub fn review_roles(&self, kind: &str) -> Option<Vec<String>> {
        let configured = self.policy.review.get(kind).cloned();
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
stemma = "0.1.0"

[members]
alice = { roles = ["maintainer", "signer"] }
bob = { roles = ["signer"] }
carol = { roles = [] }

[policy]
merge_without_approval = ["signer"]

[policy.review]
new-central = ["signer"]
"#;

    #[test]
    fn reads_members_and_policy() {
        let c = Config::parse(CONFIG).unwrap();
        assert!(c.may_merge_without_approval("bob"));
        assert!(!c.may_merge_without_approval("carol"));
        assert!(!c.may_merge_without_approval("mallory"));
        assert_eq!(c.review_roles("new-central").unwrap(), ["signer"]);
        assert_eq!(c.review_roles("policy").unwrap(), ["maintainer"]);
        assert!(c.review_roles("dependencies").is_none());
    }

    #[test]
    fn by_default_every_member_merges_their_own_work() {
        let c =
            Config::parse(&CONFIG.replace("merge_without_approval = [\"signer\"]", "")).unwrap();
        assert!(c.may_merge_without_approval("carol"));
        assert!(!c.may_merge_without_approval("mallory"));
    }
}
