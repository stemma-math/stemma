//! The report `stemma-extract` gives of a compiled library, and the signature
//! state Stemma derives from it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The report of a library, as `stemma-extract` prints it.
#[derive(Debug, Deserialize, Serialize)]
pub struct Report {
    pub version: u32,
    pub library: String,
    /// Whether the root module is a document, the library's front page.
    #[serde(rename = "rootDocument")]
    pub root_document: bool,
    pub modules: Vec<Module>,
    pub environments: Vec<Environment>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Module {
    pub name: String,
    /// `document` or `lean`.
    pub kind: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Environment {
    pub record: Record,
    /// The state of a definition or a statement.
    pub state: Option<String>,
    pub fingerprints: Option<Fingerprints>,
    pub closure: Vec<(String, String)>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub name: String,
    pub display: String,
    pub base: String,
    pub label: Option<String>,
    pub central: bool,
    pub cited: Option<String>,
    pub title: Option<String>,
    pub of: Option<String>,
    pub decls: Vec<String>,
    pub prose: String,
    pub lean: String,
    pub module: String,
    pub line: u32,
    /// The line where the environment ends. Reports of older versions lack
    /// it: the environment is then taken to be its first line.
    #[serde(rename = "endLine", default)]
    pub end_line: Option<u32>,
}

impl Record {
    /// The lines of its module's source the environment spans.
    pub fn lines(&self) -> (u32, u32) {
        (self.line, self.end_line.unwrap_or(self.line).max(self.line))
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Fingerprints {
    pub version: u32,
    pub prose: String,
    pub formal: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Diagnostic {
    pub message: String,
    pub module: Option<String>,
    pub line: Option<u32>,
}

/// The signature state of a central environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Signature {
    Unsigned,
    Signed,
    Stale,
}

/// The parts of a signature file Stemma compares: what a signature covers
/// besides the fingerprints is the environment's kind and `cited` mark.
#[derive(Debug, Deserialize)]
struct SignatureFile {
    kind: String,
    cited: Option<String>,
    fingerprints: Fingerprints,
}

impl Environment {
    /// Whether this is a central definition or statement.
    pub fn is_central_claim(&self) -> bool {
        self.record.central && matches!(self.record.base.as_str(), "definition" | "statement")
    }

    /// The signature state of a central environment, read from `signatures/`.
    pub fn signature(&self, library: &Path) -> Option<Signature> {
        if !self.is_central_claim() {
            return None;
        }
        let label = self.record.label.as_ref()?;
        let path = library.join("signatures").join(format!("{label}.toml"));
        let Ok(text) = std::fs::read_to_string(path) else {
            return Some(Signature::Unsigned);
        };
        let signed = toml::from_str::<SignatureFile>(&text).ok();
        Some(match (signed, &self.fingerprints) {
            (Some(s), Some(current))
                if &s.fingerprints == current
                    && s.kind == self.record.base
                    && s.cited == self.record.cited =>
            {
                Signature::Signed
            }
            _ => Signature::Stale,
        })
    }
}
