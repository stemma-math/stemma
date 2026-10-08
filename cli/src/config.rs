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
    pub site: Site,
}

/// The `[site]` table.
#[derive(Debug, Default, Deserialize)]
pub struct Site {
    /// Whether the workflow publishes the site to GitHub Pages from `main`.
    #[serde(default)]
    pub publish: bool,
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

/// What the library requires of a change, beyond the specification.
#[derive(Debug, Deserialize)]
pub struct Policy {
    /// Whether every central environment must carry a current signature for
    /// `stemma check` to pass. On unless the group turns it off.
    #[serde(default = "on")]
    pub require_signed_central: bool,
    /// Kinds of change, and the roles whose approval they need.
    #[serde(default)]
    pub review: BTreeMap<String, Vec<String>>,
}

fn on() -> bool {
    true
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            require_signed_central: true,
            review: BTreeMap::new(),
        }
    }
}

/// The roles that sign or approve, whose members need keys.
pub const ACTING_ROLES: &[&str] = &["signer", "maintainer"];

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

    /// The configuration at a revision of the library's repository (such as
    /// the base a change is judged against), when it has one.
    pub fn at(dir: &Path, rev: &str) -> Option<Self> {
        let out = std::process::Command::new("git")
            .args(["show", &format!("{rev}:./stemma.toml")])
            .current_dir(dir)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Self::parse(&String::from_utf8_lossy(&out.stdout)).ok()
    }

    /// The configuration that judges a change against `base`: the base's,
    /// when it has one, and the library's own otherwise (a library with no
    /// `main` yet). A change cannot alter what judges it.
    pub fn judging(dir: &Path, base: &str) -> Result<Self> {
        match Self::at(dir, base) {
            Some(config) => Ok(config),
            None => Self::read(dir),
        }
    }

    /// The members who sign or approve, by their roles, but have no key to do
    /// it with.
    pub fn keyless(&self) -> Vec<String> {
        self.members
            .iter()
            .filter(|(_, m)| {
                m.keys.is_empty() && m.roles.iter().any(|r| ACTING_ROLES.contains(&r.as_str()))
            })
            .map(|(name, _)| name.clone())
            .collect()
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
    fn signed_central_environments_are_required_unless_turned_off() {
        assert!(Config::parse(CONFIG).unwrap().policy.require_signed_central);
        let without_policy = &CONFIG[..CONFIG.find("[policy.review]").unwrap()];
        assert!(
            Config::parse(without_policy)
                .unwrap()
                .policy
                .require_signed_central
        );
        let off = format!("{without_policy}[policy]\nrequire_signed_central = false\n");
        assert!(!Config::parse(&off).unwrap().policy.require_signed_central);
    }

    #[test]
    fn finds_members_who_act_without_keys() {
        let c = Config::parse(&format!(
            "{}carol = {{ roles = [\"reader\"] }}\n",
            &CONFIG[..CONFIG.find("[policy.review]").unwrap()]
        ))
        .unwrap();
        assert_eq!(c.keyless(), ["bob"]);
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
