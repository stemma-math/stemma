//! `stemma init` and `stemma new`, which need no Lean.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stemma-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn stemma(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn init_creates_the_layout() {
    let dir = scratch("init");
    let out = stemma(
        &dir,
        &[
            "init",
            "group-theory",
            "--no-git",
            "--title",
            "Groups",
            "--member",
            "bob",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lib = dir.join("group-theory");
    for file in [
        "stemma.toml",
        "lakefile.toml",
        "lean-toolchain",
        "GroupTheory.lean",
        "AGENTS.md",
        "README.md",
        ".gitignore",
        ".github/workflows/stemma.yml",
    ] {
        assert!(lib.join(file).is_file(), "missing {file}");
    }
    let config = read(lib.join("stemma.toml"));
    assert!(config.contains("\"bob\" = { roles = [\"maintainer\", \"signer\"] }"));
    assert!(config.contains("name = \"GroupTheory\""));
    assert!(config.contains("title = \"Groups\""));
    let lakefile = read(lib.join("lakefile.toml"));
    assert!(lakefile.contains("subDir = \"lean\""));
    assert!(lakefile.contains("name = \"mathlib\""));
    assert!(
        !stemma(&dir, &["init", "group-theory", "--no-git"])
            .status
            .success()
    );
}

#[test]
fn new_adds_modules_to_the_table_of_contents() {
    let dir = scratch("new");
    assert!(
        stemma(
            &dir,
            &["init", ".", "--name", "Alg", "--no-git", "--no-mathlib"]
        )
        .status
        .success()
    );
    for args in [
        &["new", "Groups.Basic"][..],
        &["new", "Rings.Basic", "--lean"],
        &[
            "new",
            "Alg.Groups.Lagrange",
            "--title",
            "Lagrange's theorem",
        ],
        &["new", "Intro", "--after", "Stemma"],
    ] {
        let out = stemma(&dir, args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let imports: Vec<String> = read(dir.join("Alg.lean"))
        .lines()
        .filter_map(|l| l.strip_prefix("import ").map(str::to_string))
        .collect();
    assert_eq!(
        imports,
        [
            "Stemma",
            "Alg.Intro",
            "Alg.Groups.Basic",
            "Alg.Groups.Lagrange",
            "Alg.Rings.Basic"
        ]
    );
    let document = read(dir.join("Alg/Groups/Lagrange.lean"));
    assert!(document.contains("#doc (Stemma) \"Lagrange's theorem\" =>"));
    assert!(read(dir.join("Alg/Rings/Basic.lean")).starts_with("/-!\n# Basic"));
    assert!(!stemma(&dir, &["new", "Groups.Basic"]).status.success());
    assert!(!stemma(&dir, &["new", "bad name"]).status.success());
}

#[test]
fn upgrade_moves_an_old_library_to_this_version() {
    let dir = scratch("upgrade");
    let out = stemma(
        &dir,
        &[
            "init",
            ".",
            "--name",
            "Alg",
            "--title",
            "Alg",
            "--no-git",
            "--no-mathlib",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The library as stemma 0.1.0 left it.
    let config = read(dir.join("stemma.toml")).replace(
        &format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION")),
        "stemma = \"0.1.0\"",
    ) + "\n[policy]\nself_merge = [\"signer\"]\n";
    std::fs::write(dir.join("stemma.toml"), config).unwrap();
    std::fs::write(
        dir.join("AGENTS.md"),
        "# Alg\n\nThis is a Stemma library. Work on it through `stemma`: start agents with\n\
         `stemma claude` or `stemma codex`, which give them the instructions, skills and\n\
         permissions this library needs. An agent started directly is not equipped to\n\
         work here.\n",
    )
    .unwrap();
    std::fs::write(dir.join("lean-toolchain"), "leanprover/lean4:v4.20.0\n").unwrap();
    std::fs::remove_file(dir.join(".github/workflows/stemma.yml")).unwrap();

    // A dry run changes nothing.
    let out = stemma(&dir, &["upgrade", "--dry-run", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(plan["from"], "0.1.0");
    assert_eq!(plan["migrations"].as_array().unwrap().len(), 2);
    assert!(read(dir.join("stemma.toml")).contains("self_merge"));

    let out = stemma(&dir, &["upgrade", "--no-update"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let config = read(dir.join("stemma.toml"));
    assert!(config.contains(&format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION"))));
    assert!(config.contains("merge_without_approval = [\"signer\"]"));
    assert!(!config.contains("self_merge"));
    assert!(read(dir.join("AGENTS.md")).contains("Working in a Stemma library"));
    assert!(!read(dir.join("lean-toolchain")).contains("v4.20.0"));
    assert!(dir.join(".github/workflows/stemma.yml").is_file());

    // Upgrading again changes nothing.
    let out = stemma(&dir, &["upgrade", "--no-update", "--json"]);
    let again: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(again["changed"].as_array().unwrap().is_empty());
}
