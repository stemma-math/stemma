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
