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

/// A git identity, so that commits work wherever the tests run, no global
/// configuration of git or of `gh` (such as a signing key or a login), and no
/// forge: tests never reach GitHub.
const IDENTITY: [(&str, &str); 8] = [
    ("GIT_AUTHOR_NAME", "Test"),
    ("GIT_AUTHOR_EMAIL", "test@example.com"),
    ("GIT_COMMITTER_NAME", "Test"),
    ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GH_CONFIG_DIR", "/nonexistent"),
    ("STEMMA_FORGE", "none"),
];

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .envs(IDENTITY)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn stemma(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(args)
        .envs(IDENTITY)
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
    assert!(config.contains("\"bob\" = { roles = [\"maintainer\", \"signer\"], keys = [] }"));
    let workflow = read(lib.join(".github/workflows/stemma.yml"));
    assert!(workflow.contains("run: stemma verify"));
    assert!(workflow.contains("fetch-depth: 0"));
    assert!(
        !workflow.contains("token"),
        "verifying must not need the forge"
    );
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
    // From 0.1.0: two migrations of 0.2.0, and one of 0.3.0.
    assert_eq!(plan["migrations"].as_array().unwrap().len(), 3);
    assert!(read(dir.join("stemma.toml")).contains("self_merge"));

    let out = stemma(&dir, &["upgrade", "--no-update"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let config = read(dir.join("stemma.toml"));
    assert!(config.contains(&format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION"))));
    // 0.2.0 renamed the key, and 0.3.0 removed it.
    assert!(!config.contains("self_merge"));
    assert!(!config.contains("merge_without_approval"));
    assert!(read(dir.join("AGENTS.md")).contains("Working in a Stemma library"));
    assert!(!read(dir.join("lean-toolchain")).contains("v4.20.0"));
    assert!(dir.join(".github/workflows/stemma.yml").is_file());

    // Upgrading again changes nothing.
    let out = stemma(&dir, &["upgrade", "--no-update", "--json"]);
    let again: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(again["changed"].as_array().unwrap().is_empty());
}

#[test]
fn upgrade_on_main_goes_to_a_branch_of_its_own() {
    let dir = scratch("upgrade-branch");
    let out = stemma(
        &dir,
        &["init", ".", "--name", "Alg", "--no-mathlib", "--commit"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let config = read(dir.join("stemma.toml")).replace(
        &format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION")),
        "stemma = \"0.1.0\"",
    );
    std::fs::write(dir.join("stemma.toml"), config).unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Pretend to be old"]);
    let main = git(&dir, &["rev-parse", "main"]);

    let out = stemma(&dir, &["upgrade", "--no-update", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let branch = format!("upgrade/stemma-{}", env!("CARGO_PKG_VERSION"));
    assert_eq!(result["branch"], branch.as_str());
    assert_eq!(result["committed"], true);
    assert_eq!(git(&dir, &["branch", "--show-current"]), branch);
    assert_eq!(
        git(&dir, &["rev-parse", "main"]),
        main,
        "main must not move"
    );
    assert_eq!(git(&dir, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&dir, &["log", "-1", "--format=%s"]),
        format!("Upgrade to stemma {}", env!("CARGO_PKG_VERSION"))
    );
}

/// Makes an SSH key pair, returning the private key's path and the public key.
fn ssh_key(dir: &Path, name: &str) -> (String, String) {
    let path = dir.join(name);
    let out = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let public = read(path.with_extension("pub"));
    (path.display().to_string(), public.trim().to_string())
}

/// The content an approval of HEAD names: git's digest of every file but
/// signatures, as `stemma` computes it.
fn content_id(dir: &Path) -> String {
    let listing = git(dir, &["ls-tree", "-r", "--full-tree", "HEAD"]);
    let content: String = listing
        .lines()
        .filter(|l| !l.split_once('\t').unwrap().1.starts_with("signatures/"))
        .map(|l| format!("{l}\n"))
        .collect();
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();
    String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout)
        .trim()
        .to_string()
}

/// Makes an empty commit approving `kinds` of HEAD's content, signed with `key`.
fn approve(dir: &Path, key: &str, kinds: &str) {
    let content = content_id(dir);
    git(
        dir,
        &[
            "-c",
            "gpg.format=ssh",
            "-c",
            &format!("user.signingkey={key}"),
            "commit",
            "--quiet",
            "--allow-empty",
            "-S",
            "-m",
            "Approve the change",
            "-m",
            &format!("Approve: {kinds}\nApprove-content: {content}"),
        ],
    );
}

fn verify(dir: &Path) -> serde_json::Value {
    let out = stemma(dir, &["verify", "--no-build", "--base", "main", "--json"]);
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn approvals_are_signed_commits_checked_against_members_keys() {
    let dir = scratch("approvals");
    let keys = scratch("approvals-keys");
    let (alice, alice_public) = ssh_key(&keys, "alice");
    let (mallory, _) = ssh_key(&keys, "mallory");
    let out = stemma(
        &dir,
        &["init", ".", "--name", "Alg", "--no-mathlib", "--commit"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // On main, alice is the only maintainer, with her key.
    let config = read(dir.join("stemma.toml"));
    let library = &config[..config.find("[members]").unwrap()];
    std::fs::write(
        dir.join("stemma.toml"),
        format!(
            "{library}[members]\nalice = {{ roles = [\"maintainer\", \"signer\"], keys = [\"{alice_public}\"] }}\n"
        ),
    )
    .unwrap();
    git(
        &dir,
        &["commit", "--quiet", "-am", "Alice is the maintainer"],
    );

    // A change of the policy needs a maintainer's approval.
    git(&dir, &["switch", "--quiet", "--create", "change"]);
    let config = read(dir.join("stemma.toml")) + "bob = { roles = [\"signer\"] }\n";
    std::fs::write(dir.join("stemma.toml"), config).unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Add bob"]);
    let v = verify(&dir);
    assert_eq!(v["ok"], false);
    assert_eq!(v["missing"][0]["kind"], "approval");
    assert_eq!(v["missing"][0]["change"], "policy");

    // An approval signed with a key no member has does not count.
    approve(&dir, &mallory, "policy");
    let v = verify(&dir);
    assert_eq!(v["ok"], false);
    let failures = v["failures"].to_string();
    assert!(
        failures.contains("not signed with the key of a member"),
        "{failures}"
    );

    // Alice's approval does.
    approve(&dir, &alice, "policy");
    let v = verify(&dir);
    assert!(
        v["approvals"]["policy"].to_string().contains("alice"),
        "{v}"
    );
    assert!(
        !v["missing"].to_string().contains("approval"),
        "alice approved: {v}"
    );

    // A change of content after it withdraws the approval.
    std::fs::write(dir.join("README.md"), "changed\n").unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Change the README"]);
    let v = verify(&dir);
    assert!(
        v["missing"].to_string().contains("\"change\":\"policy\""),
        "{v}"
    );
}

/// Commits `files` as they are in the working tree, signed with `key`.
fn signed_commit(dir: &Path, key: &str, message: &str, files: &[&str]) {
    let mut add = vec!["add", "--"];
    add.extend(files);
    git(dir, &add);
    git(
        dir,
        &[
            "-c",
            "gpg.format=ssh",
            "-c",
            &format!("user.signingkey={key}"),
            "commit",
            "--quiet",
            "-S",
            "-m",
            message,
        ],
    );
}

#[test]
fn merges_that_only_combine_their_parents_change_nothing() {
    let dir = scratch("merges");
    let keys = scratch("merges-keys");
    let (alice, alice_public) = ssh_key(&keys, "alice");
    let out = stemma(
        &dir,
        &["init", ".", "--name", "Alg", "--no-mathlib", "--commit"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let config = read(dir.join("stemma.toml"));
    let library = &config[..config.find("[members]").unwrap()];
    std::fs::write(
        dir.join("stemma.toml"),
        format!(
            "{library}[members]\nalice = {{ roles = [\"signer\"], keys = [\"{alice_public}\"] }}\n"
        ),
    )
    .unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Alice signs"]);

    // On a branch: some work, then a signature in a commit of its own.
    git(&dir, &["switch", "--quiet", "--create", "work"]);
    std::fs::write(dir.join("README.md"), "work\n").unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Work"]);
    std::fs::create_dir_all(dir.join("signatures")).unwrap();
    std::fs::write(dir.join("signatures/one.toml"), "signer = \"alice\"\n").unwrap();
    signed_commit(&dir, &alice, "Sign one", &["signatures/one.toml"]);

    // Meanwhile main moves on, and the branch brings it in, as sharing does.
    git(&dir, &["switch", "--quiet", "main"]);
    std::fs::write(dir.join("notes.txt"), "main\n").unwrap();
    git(&dir, &["add", "notes.txt"]);
    git(&dir, &["commit", "--quiet", "-m", "Elsewhere"]);
    git(&dir, &["switch", "--quiet", "work"]);
    git(&dir, &["merge", "--quiet", "--no-edit", "main"]);
    let v = verify(&dir);
    assert_eq!(v["ok"], true, "{v}");

    // A forge checks a merge of main with the branch, made in its own name.
    git(&dir, &["switch", "--quiet", "--detach", "main"]);
    git(&dir, &["merge", "--quiet", "--no-ff", "--no-edit", "work"]);
    let v = verify(&dir);
    assert_eq!(v["ok"], true, "{v}");

    // A commit that changes signatures and anything else still fails.
    git(&dir, &["switch", "--quiet", "work"]);
    std::fs::write(dir.join("README.md"), "more work\n").unwrap();
    std::fs::write(dir.join("signatures/two.toml"), "signer = \"alice\"\n").unwrap();
    signed_commit(
        &dir,
        &alice,
        "Work and sign",
        &["README.md", "signatures/two.toml"],
    );
    let v = verify(&dir);
    assert!(
        v["failures"]
            .to_string()
            .contains("signatures go in commits of their own"),
        "{v}"
    );
}
