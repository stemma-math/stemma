//! Plans for formalizing a source (a book, a paper, notes), versioned in the
//! library so that a group shares one view of what is covered, by whom.
//!
//! A source is a directory `sources/<source>/` with a `source.toml`, which
//! describes it, and one file per part of the source (`<part>.toml`, a
//! chapter for example), which lists its items, so that members working on
//! different parts do not conflict:
//!
//! ```toml
//! # sources/burris/source.toml
//! title = "A Course in Universal Algebra"
//! cite = "BurrisSankappanavar"   # optional: its key in references.bib
//! phase = "initial"              # planning, initial or ended
//! scope = "Chapters I and II: every numbered definition and result; no exercises."
//! reuse = "Mathlib's lattices; the library's own algebras."   # optional
//! conventions = "Algebras are `Alg.Algebra σ α`."            # optional
//!
//! # sources/burris/ch1.toml
//! title = "I. Lattices"
//! owner = "alice"                # optional: the part's owner, by default its items'
//!
//! [[item]]
//! ref = "Def. I.1.1"             # where in the source; unique in the source
//! kind = "definition"            # definition, statement, proof, remark, notation, example, exercise
//! state = "done"                 # planned, in-progress, done, blocked, excluded
//! labels = ["lattice"]           # the environments that cover it
//! depends = []                   # refs of items of the same source
//! # owner = "bob"                # a member; by default the part's
//! # reason = "…"                 # why, for blocked and excluded
//! ```
//!
//! `stemma check` validates the plan, and `stemma status` shows its coverage
//! by part and by owner.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::report::Report;

/// The directory of the plans, in a library.
pub const DIR: &str = "sources";

/// The file that describes a source, in its directory.
pub const SOURCE_FILE: &str = "source.toml";

/// The states of an item.
pub const STATES: &[&str] = &["planned", "in-progress", "done", "blocked", "excluded"];

/// The kinds of an item: the base kinds of environments, and what a source
/// has besides.
pub const KINDS: &[&str] = &[
    "definition",
    "statement",
    "proof",
    "remark",
    "notation",
    "example",
    "exercise",
];

/// The phases of a source: the plan is being agreed, the initial phase (the
/// group formalizes the source, part by part), or the initial phase ended.
pub const PHASES: &[&str] = &["planning", "initial", "ended"];

/// `source.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub title: String,
    #[serde(default)]
    pub cite: Option<String>,
    pub phase: String,
    pub scope: String,
    #[serde(default)]
    pub reuse: Option<String>,
    #[serde(default)]
    pub conventions: Option<String>,
}

/// A part's file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartFile {
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default, rename = "item")]
    pub items: Vec<Item>,
}

/// An item of a source.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: String,
    pub state: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// A part, as read.
#[derive(Debug)]
pub struct Part {
    /// Its file's name, without `.toml`.
    pub name: String,
    pub file: PartFile,
}

/// A source, as read.
#[derive(Debug)]
pub struct Source {
    /// Its directory's name.
    pub name: String,
    pub file: Option<SourceFile>,
    pub parts: Vec<Part>,
}

/// The plans of a library, and the problems reading them.
#[derive(Debug, Default)]
pub struct Plans {
    pub sources: Vec<Source>,
    pub problems: Vec<String>,
}

/// Reads every plan of the library at `dir`. A file that cannot be read is a
/// problem, and is left out.
pub fn read(dir: &Path) -> Plans {
    let mut plans = Plans::default();
    let root = dir.join(DIR);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return plans;
    };
    let mut dirs: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for path in dirs {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut source = Source {
            name: name.clone(),
            file: None,
            parts: Vec::new(),
        };
        let mut files: Vec<_> = std::fs::read_dir(&path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "toml"))
            .collect();
        files.sort();
        for file in files {
            let file_name = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let shown = format!("{DIR}/{name}/{file_name}");
            let text = match std::fs::read_to_string(&file) {
                Ok(text) => text,
                Err(error) => {
                    plans.problems.push(format!("{shown}: {error}."));
                    continue;
                }
            };
            if file_name == SOURCE_FILE {
                match toml::from_str::<SourceFile>(&text) {
                    Ok(f) => source.file = Some(f),
                    Err(error) => plans.problems.push(toml_problem(&shown, &error)),
                }
            } else {
                match toml::from_str::<PartFile>(&text) {
                    Ok(f) => source.parts.push(Part {
                        name: file_name.trim_end_matches(".toml").to_string(),
                        file: f,
                    }),
                    Err(error) => plans.problems.push(toml_problem(&shown, &error)),
                }
            }
        }
        plans.sources.push(source);
    }
    plans
}

/// A TOML error, in one line.
fn toml_problem(file: &str, error: &toml::de::Error) -> String {
    let message = error.message().trim_end_matches('.');
    match error.span() {
        Some(span) => format!("{file}: {message} (at byte {}).", span.start),
        None => format!("{file}: {message}."),
    }
}

/// `one of a, b or c`.
fn one_of(values: &[&str]) -> String {
    match values.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.to_string(),
        None => String::new(),
    }
}

impl Item {
    /// Its owner: its own, or its part's.
    pub fn owner<'a>(&'a self, part: &'a PartFile) -> Option<&'a str> {
        self.owner.as_deref().or(part.owner.as_deref())
    }
}

/// The keys of the entries of a BibTeX file, read loosely: what follows
/// `@<type>{` up to the first comma.
fn bib_keys(text: &str) -> HashSet<&str> {
    text.split('@')
        .skip(1)
        .filter_map(|entry| {
            let (_, rest) = entry.split_once('{')?;
            let (key, _) = rest.split_once(',')?;
            Some(key.trim())
        })
        .collect()
}

/// The problems of the plans: unknown states, kinds and phases, labels that
/// name no environment (when the library's report is known), owners who are
/// not members, dependencies that do not exist or form a cycle, and a `cite`
/// that is not a key of `references` (the text of `references.bib`).
pub fn check(
    plans: &Plans,
    members: &[String],
    report: Option<&Report>,
    references: &str,
) -> Vec<String> {
    let keys = bib_keys(references);
    let labels: Option<HashSet<&str>> = report.map(|r| {
        r.environments
            .iter()
            .filter_map(|e| e.record.label.as_deref())
            .collect()
    });
    let mut problems = plans.problems.clone();
    for source in &plans.sources {
        let dir = format!("{DIR}/{}", source.name);
        match &source.file {
            None if !plans
                .problems
                .iter()
                .any(|p| p.starts_with(&format!("{dir}/{SOURCE_FILE}:"))) =>
            {
                problems.push(format!(
                    "{dir} has no {SOURCE_FILE}, which says what the source is and its scope."
                ));
            }
            Some(file) => {
                if !PHASES.contains(&file.phase.as_str()) {
                    problems.push(format!(
                        "{dir}/{SOURCE_FILE}: unknown phase '{}': it is one of {}.",
                        file.phase,
                        one_of(PHASES)
                    ));
                }
                if file.scope.trim().is_empty() {
                    problems.push(format!(
                        "{dir}/{SOURCE_FILE}: the scope is empty: say what is formalized."
                    ));
                }
                if let Some(cite) = &file.cite
                    && !keys.contains(cite.as_str())
                {
                    problems.push(format!(
                        "{dir}/{SOURCE_FILE}: '{cite}' is not a key of references.bib."
                    ));
                }
            }
            None => {}
        }
        // Where each ref is, to find repeated and missing ones.
        let mut at: HashMap<&str, &str> = HashMap::new();
        for part in &source.parts {
            let file = format!("{dir}/{}.toml", part.name);
            if let Some(owner) = &part.file.owner
                && !members.contains(owner)
            {
                problems.push(format!(
                    "{file}: the owner '{owner}' is not a member of the library (stemma.toml)."
                ));
            }
            for item in &part.file.items {
                let what = format!("{file}: item '{}'", item.reference);
                match at.get(item.reference.as_str()) {
                    Some(other) if *other == part.name => {
                        problems.push(format!("{what} is listed twice."));
                    }
                    Some(other) => problems.push(format!(
                        "{what} is also listed in {dir}/{other}.toml: refs are unique in a source."
                    )),
                    None => {
                        at.insert(&item.reference, &part.name);
                    }
                }
                if !STATES.contains(&item.state.as_str()) {
                    problems.push(format!(
                        "{what}: unknown state '{}': it is one of {}.",
                        item.state,
                        one_of(STATES)
                    ));
                }
                if !KINDS.contains(&item.kind.as_str()) {
                    problems.push(format!(
                        "{what}: unknown kind '{}': it is one of {}.",
                        item.kind,
                        one_of(KINDS)
                    ));
                }
                let reason = item.reason.as_deref().is_some_and(|r| !r.trim().is_empty());
                if matches!(item.state.as_str(), "blocked" | "excluded") && !reason {
                    problems.push(format!("{what} is {}: say why, in `reason`.", item.state));
                }
                if item.state == "done" && item.labels.is_empty() {
                    problems.push(format!(
                        "{what} is done: name the environments that cover it, in `labels`."
                    ));
                }
                match item.owner(&part.file) {
                    None if item.state == "in-progress" => problems.push(format!(
                        "{what} is in progress: name its owner, in `owner`."
                    )),
                    Some(owner) if item.owner.is_some() && !members.contains(&owner.into()) => {
                        problems.push(format!(
                            "{what}: the owner '{owner}' is not a member of the library \
                             (stemma.toml)."
                        ))
                    }
                    _ => {}
                }
                if let Some(labels) = &labels {
                    for label in &item.labels {
                        if !labels.contains(label.as_str()) {
                            problems
                                .push(format!("{what}: no environment has the label '{label}'."));
                        }
                    }
                }
            }
        }
        // Dependencies.
        let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for part in &source.parts {
            for item in &part.file.items {
                for dep in &item.depends {
                    if !at.contains_key(dep.as_str()) {
                        problems.push(format!(
                            "{dir}/{}.toml: item '{}' depends on '{dep}', which is no item of \
                             this source.",
                            part.name, item.reference
                        ));
                    }
                }
                graph
                    .entry(&item.reference)
                    .or_default()
                    .extend(item.depends.iter().map(String::as_str));
            }
        }
        for cycle in cycles(&graph) {
            problems.push(format!(
                "{dir}: the dependencies form a cycle: {}.",
                cycle.join(" → ")
            ));
        }
    }
    problems
}

/// The cycles of a graph, each once, as the path that closes it.
fn cycles<'a>(graph: &BTreeMap<&'a str, Vec<&'a str>>) -> Vec<Vec<&'a str>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Open,
        Done,
    }
    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        marks: &mut HashMap<&'a str, Mark>,
        path: &mut Vec<&'a str>,
        found: &mut Vec<Vec<&'a str>>,
    ) {
        marks.insert(node, Mark::Open);
        path.push(node);
        for &next in graph.get(node).into_iter().flatten() {
            match marks.get(next) {
                Some(Mark::Open) => {
                    let start = path.iter().position(|n| *n == next).unwrap_or(0);
                    let mut cycle = path[start..].to_vec();
                    cycle.push(next);
                    found.push(cycle);
                }
                Some(Mark::Done) => {}
                None if graph.contains_key(next) => visit(next, graph, marks, path, found),
                None => {}
            }
        }
        path.pop();
        marks.insert(node, Mark::Done);
    }
    let mut marks = HashMap::new();
    let mut found = Vec::new();
    for &node in graph.keys() {
        if !marks.contains_key(node) {
            visit(node, graph, &mut marks, &mut Vec::new(), &mut found);
        }
    }
    found
}

/// How many items are in each state, and how many of those done are proved.
#[derive(Debug, Default, Serialize, Clone)]
pub struct Coverage {
    pub items: usize,
    pub planned: usize,
    pub in_progress: usize,
    pub done: usize,
    /// Items done whose environments are all proved: no `sorry`, no
    /// assumption accepted from the literature, nothing left unformalized.
    pub proved: usize,
    pub blocked: usize,
    pub excluded: usize,
}

impl Coverage {
    fn add(&mut self, item: &Item, proved: bool) {
        self.items += 1;
        match item.state.as_str() {
            "planned" => self.planned += 1,
            "in-progress" => self.in_progress += 1,
            "done" => {
                self.done += 1;
                if proved {
                    self.proved += 1;
                }
            }
            "blocked" => self.blocked += 1,
            "excluded" => self.excluded += 1,
            _ => {}
        }
    }

    /// One line, for people.
    pub fn line(&self) -> String {
        format!(
            "{} items · {} done ({} proved) · {} in progress · {} planned · {} blocked · {} \
             excluded",
            self.items,
            self.done,
            self.proved,
            self.in_progress,
            self.planned,
            self.blocked,
            self.excluded
        )
    }
}

/// The coverage of a part.
#[derive(Debug, Serialize)]
pub struct PartCoverage {
    pub part: String,
    pub title: String,
    #[serde(flatten)]
    pub coverage: Coverage,
}

/// The coverage of an owner, in a source.
#[derive(Debug, Serialize)]
pub struct OwnerCoverage {
    /// The member, or `None` for items nobody owns.
    pub owner: Option<String>,
    #[serde(flatten)]
    pub coverage: Coverage,
}

/// The coverage of a source, by part and by owner.
#[derive(Debug, Serialize)]
pub struct SourceCoverage {
    pub source: String,
    pub title: Option<String>,
    pub phase: Option<String>,
    pub scope: Option<String>,
    pub cite: Option<String>,
    pub reuse: Option<String>,
    pub conventions: Option<String>,
    #[serde(flatten)]
    pub coverage: Coverage,
    pub parts: Vec<PartCoverage>,
    pub owners: Vec<OwnerCoverage>,
}

/// The coverage of every source. An item done counts as proved only when
/// every environment it names is proved in the report.
pub fn coverage(plans: &Plans, report: &Report) -> Vec<SourceCoverage> {
    let states: HashMap<&str, Option<&str>> = report
        .environments
        .iter()
        .filter_map(|e| Some((e.record.label.as_deref()?, e.state.as_deref())))
        .collect();
    let proved = |item: &Item| {
        !item.labels.is_empty()
            && item
                .labels
                .iter()
                .all(|l| states.get(l.as_str()).copied().flatten() == Some("proved"))
    };
    plans
        .sources
        .iter()
        .map(|source| {
            let mut total = Coverage::default();
            let mut owners: BTreeMap<Option<String>, Coverage> = BTreeMap::new();
            let parts = source
                .parts
                .iter()
                .map(|part| {
                    let mut coverage = Coverage::default();
                    for item in &part.file.items {
                        let p = proved(item);
                        coverage.add(item, p);
                        total.add(item, p);
                        owners
                            .entry(item.owner(&part.file).map(str::to_string))
                            .or_default()
                            .add(item, p);
                    }
                    PartCoverage {
                        part: part.name.clone(),
                        title: part.file.title.clone(),
                        coverage,
                    }
                })
                .collect();
            SourceCoverage {
                source: source.name.clone(),
                title: source.file.as_ref().map(|f| f.title.clone()),
                phase: source.file.as_ref().map(|f| f.phase.clone()),
                scope: source.file.as_ref().map(|f| f.scope.clone()),
                cite: source.file.as_ref().and_then(|f| f.cite.clone()),
                reuse: source.file.as_ref().and_then(|f| f.reuse.clone()),
                conventions: source.file.as_ref().and_then(|f| f.conventions.clone()),
                coverage: total,
                parts,
                owners: owners
                    .into_iter()
                    .map(|(owner, coverage)| OwnerCoverage { owner, coverage })
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str) -> Part {
        Part {
            name: "ch1".into(),
            file: toml::from_str(text).unwrap(),
        }
    }

    fn plans(parts: Vec<Part>) -> Plans {
        Plans {
            sources: vec![Source {
                name: "book".into(),
                file: Some(
                    toml::from_str("title = \"B\"\nphase = \"initial\"\nscope = \"All.\"\n")
                        .unwrap(),
                ),
                parts,
            }],
            problems: Vec::new(),
        }
    }

    fn report(labels: &[(&str, &str)]) -> Report {
        let environments: Vec<serde_json::Value> = labels
            .iter()
            .map(|(label, state)| {
                serde_json::json!({
                    "record": {
                        "name": "theorem", "display": "Theorem", "base": "statement",
                        "label": label, "central": false, "cited": null, "title": null,
                        "of": null, "decls": [], "prose": "", "lean": "",
                        "module": "Alg.A", "line": 1,
                    },
                    "state": state, "fingerprints": null, "closure": [],
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "version": 1, "library": "Alg", "rootDocument": false, "modules": [],
            "environments": environments, "diagnostics": [],
        }))
        .unwrap()
    }

    const GOOD: &str = r#"
title = "I"
owner = "alice"

[[item]]
ref = "Def. 1"
kind = "definition"
state = "done"
labels = ["one"]

[[item]]
ref = "Thm. 2"
kind = "statement"
state = "in-progress"
labels = ["two"]
depends = ["Def. 1"]

[[item]]
ref = "Ex. 3"
kind = "exercise"
state = "excluded"
reason = "Exercises are out of scope."
"#;

    #[test]
    fn a_good_plan_has_no_problems() {
        let p = plans(vec![part(GOOD)]);
        let r = report(&[("one", "proved"), ("two", "pending")]);
        assert_eq!(
            check(&p, &["alice".into()], Some(&r), ""),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unknown_states_labels_owners_and_dependencies_are_problems() {
        let text = GOOD
            .replace("state = \"in-progress\"", "state = \"started\"")
            .replace("[\"one\"]", "[\"missing\"]")
            .replace("depends = [\"Def. 1\"]", "depends = [\"Def. 9\"]")
            .replace("owner = \"alice\"", "owner = \"carol\"");
        let p = plans(vec![part(&text)]);
        let r = report(&[("one", "proved"), ("two", "pending")]);
        let problems = check(&p, &["alice".into()], Some(&r), "").join("\n");
        assert!(problems.contains("unknown state 'started'"), "{problems}");
        assert!(
            problems.contains("no environment has the label 'missing'"),
            "{problems}"
        );
        assert!(
            problems.contains("depends on 'Def. 9', which is no item"),
            "{problems}"
        );
        assert!(
            problems.contains("the owner 'carol' is not a member"),
            "{problems}"
        );
        // Without a report, labels are not checked.
        assert!(
            !check(&p, &["alice".into()], None, "")
                .join("\n")
                .contains("label")
        );
    }

    #[test]
    fn a_cycle_of_dependencies_is_a_problem() {
        let text = GOOD.replace(
            "ref = \"Def. 1\"\nkind = \"definition\"",
            "ref = \"Def. 1\"\nkind = \"definition\"\ndepends = [\"Thm. 2\"]",
        );
        let p = plans(vec![part(&text)]);
        let problems = check(&p, &["alice".into()], None, "");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains("form a cycle: Def. 1 → Thm. 2 → Def. 1"),
            "{problems:?}"
        );
    }

    #[test]
    fn blocked_and_done_items_say_why_and_what() {
        let text = GOOD
            .replace("reason = \"Exercises are out of scope.\"", "")
            .replace("labels = [\"one\"]", "");
        let problems = check(&plans(vec![part(&text)]), &["alice".into()], None, "").join("\n");
        assert!(problems.contains("is excluded: say why"), "{problems}");
        assert!(
            problems.contains("is done: name the environments"),
            "{problems}"
        );
    }

    #[test]
    fn coverage_counts_proved_items_apart() {
        let p = plans(vec![part(GOOD)]);
        let c = &coverage(&p, &report(&[("one", "pending"), ("two", "pending")]))[0];
        assert_eq!(
            (c.coverage.items, c.coverage.done, c.coverage.proved),
            (3, 1, 0)
        );
        let c = &coverage(&p, &report(&[("one", "proved")]))[0];
        assert_eq!(c.coverage.proved, 1);
        assert_eq!(c.parts[0].coverage.in_progress, 1);
        assert_eq!(c.owners.len(), 1);
        assert_eq!(c.owners[0].owner.as_deref(), Some("alice"));
    }

    #[test]
    fn files_are_read_strictly() {
        let dir = std::env::temp_dir().join(format!("stemma-sources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sources/book")).unwrap();
        std::fs::write(dir.join("sources/book/ch1.toml"), GOOD).unwrap();
        std::fs::write(
            dir.join("sources/book/ch2.toml"),
            "title = \"II\"\nunknown = 1\n",
        )
        .unwrap();
        let p = read(&dir);
        assert_eq!(p.sources[0].parts.len(), 1);
        let problems = check(&p, &["alice".into()], None, "").join("\n");
        assert!(
            problems.contains("sources/book/ch2.toml: unknown field"),
            "{problems}"
        );
        assert!(
            problems.contains("sources/book has no source.toml"),
            "{problems}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
