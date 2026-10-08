//! The person's review of read-backs: marks, notes and archiving, kept in
//! `.stemma/reviews.json`.
//!
//! This is the person's own record of what they have looked at: local to one
//! clone, never committed, never signed, and no part of git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The longest note the page accepts, in characters.
pub const NOTE_LIMIT: usize = 2000;

/// How a person has marked a read-back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    #[default]
    Unread,
    Read,
    Approved,
}

/// The review of one read-back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Review {
    #[serde(default)]
    pub mark: Mark,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The formal fingerprint of the read-back the mark and the note were
    /// made on: when the read-back changes, they are outdated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub formal: String,
    /// The formal fingerprint of the read-back that was archived, if any: a
    /// read-back made of other Lean is not archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<String>,
}

impl Review {
    /// Whether the mark and the note were made on another read-back than the
    /// one with the formal fingerprint `formal`.
    pub fn outdated(&self, formal: &str) -> bool {
        (self.mark != Mark::Unread || !self.note.is_empty()) && self.formal != formal
    }

    /// Whether the read-back with the formal fingerprint `formal` is archived.
    pub fn is_archived(&self, formal: &str) -> bool {
        self.archived.as_deref() == Some(formal)
    }

    fn is_empty(&self) -> bool {
        self.mark == Mark::Unread && self.note.is_empty() && self.archived.is_none()
    }
}

/// A change the page asks for, on the read-back with a given fingerprint.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub label: String,
    #[serde(default)]
    pub mark: Option<Mark>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
}

/// Every review of a clone, as kept in `.stemma/reviews.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Reviews {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub readbacks: BTreeMap<String, Review>,
}

fn version() -> u32 {
    1
}

/// Where the reviews of a library are kept.
pub fn path(library: &Path) -> PathBuf {
    library.join(".stemma").join("reviews.json")
}

impl Reviews {
    /// Reads the reviews of a library; none when there is no file yet.
    pub fn load(library: &Path) -> Result<Self> {
        let path = path(library);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                version: version(),
                ..Self::default()
            }),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Writes the reviews of a library, replacing the file in one step.
    pub fn save(&self, library: &Path) -> Result<()> {
        let path = path(library);
        let dir = path.parent().expect("the reviews live in .stemma/");
        std::fs::create_dir_all(dir)?;
        let temporary = dir.join("reviews.json.tmp");
        std::fs::write(&temporary, serde_json::to_string_pretty(self)? + "\n")?;
        std::fs::rename(&temporary, &path).with_context(|| format!("writing {}", path.display()))
    }

    /// The review of a label, or an empty one.
    pub fn get(&self, label: &str) -> Review {
        self.readbacks.get(label).cloned().unwrap_or_default()
    }

    /// Applies a change to the read-back with the formal fingerprint `formal`.
    ///
    /// A new mark or note is made on that read-back. A note changed without a
    /// mark, on a read-back whose mark is outdated, leaves it unread: the old
    /// mark was about other Lean.
    pub fn apply(&mut self, change: &Change, formal: &str) -> Result<Review> {
        if let Some(note) = &change.note
            && note.chars().count() > NOTE_LIMIT
        {
            bail!("a note has at most {NOTE_LIMIT} characters");
        }
        let mut review = self.get(&change.label);
        if change.mark.is_some() || change.note.is_some() {
            if review.formal != formal {
                review.mark = Mark::Unread;
                review.formal = formal.to_string();
            }
            if let Some(mark) = change.mark {
                review.mark = mark;
            }
            if let Some(note) = &change.note {
                review.note = note.trim().to_string();
            }
        }
        match change.archived {
            Some(true) => review.archived = Some(formal.to_string()),
            Some(false) => review.archived = None,
            None => {}
        }
        if review.is_empty() {
            self.readbacks.remove(&change.label);
        } else {
            self.readbacks.insert(change.label.clone(), review.clone());
        }
        Ok(review)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(label: &str) -> Change {
        Change {
            label: label.into(),
            ..Change::default()
        }
    }

    #[test]
    fn marks_belong_to_the_read_back_they_were_made_on() {
        let mut reviews = Reviews::default();
        let approve = Change {
            mark: Some(Mark::Approved),
            note: Some("  Check the hypothesis.  ".into()),
            ..change("x")
        };
        let review = reviews.apply(&approve, "sha256:a").unwrap();
        assert_eq!(review.mark, Mark::Approved);
        assert_eq!(review.note, "Check the hypothesis.");
        assert!(!review.outdated("sha256:a"));
        // A read-back of other Lean: the mark is outdated.
        assert!(review.outdated("sha256:b"));
        // A note on the new read-back does not carry the old approval over.
        let note = Change {
            note: Some("Better now.".into()),
            ..change("x")
        };
        let review = reviews.apply(&note, "sha256:b").unwrap();
        assert_eq!(review.mark, Mark::Unread);
        assert_eq!(review.formal, "sha256:b");
        assert!(!review.outdated("sha256:b"));
    }

    #[test]
    fn archiving_follows_the_fingerprint() {
        let mut reviews = Reviews::default();
        let archive = Change {
            archived: Some(true),
            ..change("x")
        };
        let review = reviews.apply(&archive, "sha256:a").unwrap();
        assert!(review.is_archived("sha256:a"));
        assert!(!review.is_archived("sha256:b"));
        let unarchive = Change {
            archived: Some(false),
            ..change("x")
        };
        reviews.apply(&unarchive, "sha256:a").unwrap();
        // An empty review leaves nothing behind.
        assert!(reviews.readbacks.is_empty());
    }

    #[test]
    fn notes_are_short() {
        let mut reviews = Reviews::default();
        let long = Change {
            note: Some("x".repeat(NOTE_LIMIT + 1)),
            ..change("x")
        };
        assert!(reviews.apply(&long, "sha256:a").is_err());
        assert!(reviews.readbacks.is_empty());
    }

    #[test]
    fn reviews_round_trip_through_their_file() {
        let dir = std::env::temp_dir().join(format!("stemma-reviews-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut reviews = Reviews::load(&dir).unwrap();
        assert!(reviews.readbacks.is_empty());
        let read = Change {
            mark: Some(Mark::Read),
            ..change("x")
        };
        reviews.apply(&read, "sha256:a").unwrap();
        reviews.save(&dir).unwrap();
        let again = Reviews::load(&dir).unwrap();
        assert_eq!(again.version, 1);
        assert_eq!(again.get("x").mark, Mark::Read);
        assert!(path(&dir).is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
