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

// Branches and sharing. The group's repository is a local bare repository, and
// the forge a file (`STEMMA_FORGE=file:…`): nothing reaches GitHub.

/// A library whose `main` is on a bare repository, its clone, and the file of
/// its forge, where the person is `alice`.
struct Shared {
    root: PathBuf,
    lib: PathBuf,
    remote: PathBuf,
    forge: PathBuf,
}

impl Shared {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        let remote = root.join("remote.git");
        git(&root, &["init", "--quiet", "--bare", "remote.git"]);
        let forge = root.join("forge.json");
        std::fs::write(&forge, "{\"login\": \"alice\"}").unwrap();
        let shared = Self {
            lib: root.join("lib"),
            root,
            remote,
            forge,
        };
        let out = shared.stemma(
            &shared.root,
            &[
                "init",
                "lib",
                "--name",
                "Alg",
                "--no-mathlib",
                "--commit",
                "--remote",
                shared.remote.to_str().unwrap(),
                "--push",
                "--yes",
            ],
            &[],
        );
        assert!(out.status.success(), "{}", stderr(&out));
        // A fake agent, so that starting one only chooses the branch.
        let bin = shared.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for agent in ["claude", "codex"] {
            let path = bin.join(agent);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        shared
    }

    /// Runs stemma in `dir` with this forge, and `env`.
    fn stemma(&self, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(env!("CARGO_BIN_EXE_stemma"))
            .args(args)
            .envs(IDENTITY)
            .env("STEMMA_FORGE", format!("file:{}", self.forge.display()))
            .env("PATH", path)
            .env_remove("STEMMA_SESSION")
            .envs(env.iter().copied())
            .current_dir(dir)
            .output()
            .unwrap()
    }

    /// `stemma share --no-build --json` in the library, with `args`.
    fn share(&self, args: &[&str], env: &[(&str, &str)]) -> serde_json::Value {
        let mut all = vec!["share", "--no-build", "--json"];
        all.extend(args);
        let out = self.stemma(&self.lib, &all, env);
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("{}{}", String::from_utf8_lossy(&out.stdout), stderr(&out)))
    }

    /// Where a branch is on the group's repository.
    fn remote_branch(&self, branch: &str) -> Option<String> {
        let out = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", branch])
            .current_dir(&self.remote)
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn pull_requests(&self) -> Vec<serde_json::Value> {
        let state: serde_json::Value = serde_json::from_str(&read(self.forge.clone())).unwrap();
        state["pull_requests"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Commits a new file on the current branch, and returns the commit.
    fn commit(&self, dir: &Path, file: &str) -> String {
        std::fs::write(dir.join(file), format!("{file}\n")).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "--quiet", "-m", &format!("Add {file}")]);
        git(dir, &["rev-parse", "HEAD"])
    }

    /// Another clone of the group's repository, as another machine or person.
    fn clone(&self, name: &str) -> PathBuf {
        git(
            &self.root,
            &[
                "clone",
                "--quiet",
                "--branch",
                "main",
                self.remote.to_str().unwrap(),
                name,
            ],
        );
        self.root.join(name)
    }
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn share_moves_the_share_branch_forward_and_never_commits_on_it() {
    let s = Shared::new("share-forward");
    let out = s.stemma(&s.lib, &["claude"], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(git(&s.lib, &["branch", "--show-current"]), "work/alice");
    let first = s.commit(&s.lib, "a.txt");
    let v = s.share(&["--title", "First"], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["share_branch"], "share/alice");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        s.remote_branch("main"),
        Some(git(&s.lib, &["rev-parse", "main"]))
    );
    let prs = s.pull_requests();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["head"], "share/alice");
    assert_eq!(prs[0]["title"], "First");
    // Without a personal remote, sharing pushes only the share branch.
    assert_eq!(s.remote_branch("work/alice"), None);

    // More work: the open pull request is updated, and share/alice moves
    // forward to HEAD, a commit of work/alice.
    s.commit(&s.lib, "b.txt");
    let third = s.commit(&s.lib, "c.txt");
    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["updated"], true);
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(third.as_str())
    );
    assert_eq!(s.pull_requests().len(), 1);
    assert!(!git(&s.lib, &["branch", "--list", "share/*"]).contains("share"));

    // Up to an earlier commit: not one already shared, nor one of another branch.
    let fourth = s.commit(&s.lib, "d.txt");
    std::fs::create_dir_all(s.lib.join("signatures")).unwrap();
    s.commit(&s.lib, "signatures/x.toml");
    let out = s.stemma(&s.lib, &["share", "--no-build", "--upto", &first], &[]);
    assert!(
        stderr(&out).contains("already goes beyond it"),
        "{}",
        stderr(&out)
    );
    let v = s.share(&["--upto", &fourth], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(fourth.as_str())
    );
    assert_eq!(v["left_out"].as_array().unwrap().len(), 1, "{v}");
    let out = s.stemma(&s.lib, &["share", "--no-build", "--upto", "main~0"], &[]);
    assert!(!out.status.success());

    // After a merge, the next share moves share/alice forward and opens a new
    // pull request.
    let other = s.clone("merger");
    git(
        &other,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "--no-edit",
            "origin/share/alice",
        ],
    );
    git(&other, &["push", "--quiet", "origin", "main"]);
    let forge = read(s.forge.clone()).replace("\"open\"", "\"merged\"");
    std::fs::write(&s.forge, forge).unwrap();
    s.commit(&s.lib, "e.txt");
    let v = s.share(&["--title", "Second"], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["updated"], false);
    // Main came in on the working branch, and share/alice moved to it.
    assert_eq!(
        s.remote_branch("share/alice"),
        Some(git(&s.lib, &["rev-parse", "HEAD"]))
    );
    assert_eq!(
        git(&s.lib, &["log", "-1", "--format=%p"])
            .split(' ')
            .count(),
        2
    );
    let prs = s.pull_requests();
    assert_eq!(prs.len(), 2);
    assert_eq!(prs[1]["state"], "open");
}

#[test]
fn foreign_commits_on_the_share_branch_are_never_overwritten_silently() {
    let s = Shared::new("share-foreign");
    s.stemma(&s.lib, &["claude"], &[]);
    s.commit(&s.lib, "a.txt");
    assert_eq!(s.share(&[], &[])["ok"], true);

    // The forge's "Update branch", or a suggestion accepted on the forge.
    let other = s.clone("forge");
    git(&other, &["switch", "--quiet", "share/alice"]);
    let foreign = s.commit(&other, "suggestion.txt");
    git(&other, &["push", "--quiet", "origin", "share/alice"]);
    s.commit(&s.lib, "b.txt");

    // Without interaction, sharing stops, explains, and names the flag.
    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["pushed"], false);
    assert!(
        v["stopped"].as_str().unwrap().contains("--foreign merge"),
        "{v}"
    );
    assert_eq!(v["foreign"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(foreign.as_str())
    );

    // An agent never discards them.
    let v = s.share(&["--foreign", "discard"], &[("STEMMA_SESSION", "1")]);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(foreign.as_str())
    );

    // Bringing them in keeps them, and share/alice moves forward.
    let v = s.share(&["--foreign", "merge"], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["merged_foreign"], true);
    assert!(s.lib.join("suggestion.txt").is_file());
    assert_eq!(
        s.remote_branch("share/alice"),
        Some(git(&s.lib, &["rev-parse", "HEAD"]))
    );

    // Again, and this time a person discards them.
    git(
        &other,
        &["pull", "--quiet", "--no-rebase", "origin", "share/alice"],
    );
    let foreign = s.commit(&other, "unwanted.txt");
    git(&other, &["push", "--quiet", "origin", "share/alice"]);
    let v = s.share(&["--foreign", "discard"], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["replaced"], true);
    let shared = s.remote_branch("share/alice").unwrap();
    assert_ne!(shared, foreign);
    assert_eq!(shared, git(&s.lib, &["rev-parse", "HEAD"]));
}

#[test]
fn two_machines_compare_with_the_remote_share_branch() {
    let s = Shared::new("share-machines");
    s.stemma(&s.lib, &["claude"], &[]);
    s.commit(&s.lib, "a.txt");
    git(&s.lib, &["push", "--quiet", "origin", "work/alice"]);
    assert_eq!(s.share(&[], &[])["ok"], true);

    // The same person, on another machine, shares more.
    let laptop = s.clone("laptop");
    git(&laptop, &["switch", "--quiet", "work/alice"]);
    let later = s.commit(&laptop, "b.txt");
    let out = s.stemma(&laptop, &["share", "--no-build", "--json"], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(later.as_str())
    );

    // The first machine, whose own copy of share/alice is behind, does not
    // overwrite it.
    s.commit(&s.lib, "c.txt");
    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], false, "{v}");
    assert!(v["stopped"].as_str().unwrap().contains("--foreign"), "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(later.as_str())
    );
}

#[test]
fn a_rewritten_working_branch_replaces_its_share_branch_only_when_told() {
    let s = Shared::new("share-rewrite");
    s.stemma(&s.lib, &["claude"], &[]);
    s.commit(&s.lib, "a.txt");
    let shared = s.commit(&s.lib, "b.txt");
    assert_eq!(s.share(&[], &[])["ok"], true);
    git(
        &s.lib,
        &["commit", "--quiet", "--amend", "-m", "Add b, better"],
    );

    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], false, "{v}");
    assert!(v["stopped"].as_str().unwrap().contains("--yes"), "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(shared.as_str())
    );
    // An agent never forces a push.
    let v = s.share(&["--yes"], &[("STEMMA_SESSION", "1")]);
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(shared.as_str())
    );

    let v = s.share(&["--yes"], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["replaced"], true);
    assert_eq!(
        s.remote_branch("share/alice"),
        Some(git(&s.lib, &["rev-parse", "HEAD"]))
    );

    // A rewrite with someone else's commit on top is foreign, not rewritten.
    let other = s.clone("other");
    git(&other, &["switch", "--quiet", "share/alice"]);
    let foreign = s.commit(&other, "theirs.txt");
    git(&other, &["push", "--quiet", "origin", "share/alice"]);
    git(
        &s.lib,
        &["commit", "--quiet", "--amend", "-m", "Add b, once more"],
    );
    let v = s.share(&["--yes"], &[]);
    assert_eq!(v["ok"], false, "{v}");
    assert!(v["stopped"].as_str().unwrap().contains("--foreign"), "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(foreign.as_str())
    );
}

#[test]
fn an_open_pull_request_from_the_working_branch_finishes_first() {
    // As stemma 0.3 left it: a pull request from work/alice itself.
    let s = Shared::new("share-legacy");
    s.stemma(&s.lib, &["claude"], &[]);
    s.commit(&s.lib, "a.txt");
    git(&s.lib, &["push", "--quiet", "origin", "work/alice"]);
    std::fs::write(
        &s.forge,
        "{\"login\": \"alice\", \"pull_requests\": [{\"number\": 1, \"url\": \"pr/1\", \
         \"head\": \"work/alice\", \"base\": \"main\", \"state\": \"open\", \"title\": \"Old\"}]}",
    )
    .unwrap();
    let head = s.commit(&s.lib, "b.txt");
    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["from_working_branch"], true);
    assert_eq!(v["pull_request"], "pr/1");
    assert_eq!(
        s.remote_branch("work/alice").as_deref(),
        Some(head.as_str())
    );
    assert_eq!(s.remote_branch("share/alice"), None);
    assert_eq!(s.pull_requests().len(), 1);
}

#[test]
fn agents_start_on_working_branches_chosen_with_flags() {
    let s = Shared::new("agent-branches");
    let run = |args: &[&str]| s.stemma(&s.lib, args, &[]);
    // Never a share branch, never main, never work/<person>/<topic>.
    for bad in [
        &["claude", "--branch", "share/alice"][..],
        &["claude", "--branch", "main"],
        &["claude", "--branch", "work/alice/groups"],
        &["claude", "--branch", "groups"],
    ] {
        let out = run(bad);
        assert!(!out.status.success(), "{bad:?}");
    }
    let out = run(&["claude", "--branch", "work/alice/groups"]);
    assert!(
        stderr(&out).contains("never work/<person>/<topic>"),
        "{}",
        stderr(&out)
    );

    // A new branch starts from an up-to-date main, not from HEAD.
    git(&s.lib, &["switch", "--quiet", "--create", "elsewhere"]);
    s.commit(&s.lib, "elsewhere.txt");
    let out = run(&["codex", "--branch", "work/alice-groups"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        git(&s.lib, &["branch", "--show-current"]),
        "work/alice-groups"
    );
    assert_eq!(
        git(&s.lib, &["rev-parse", "HEAD"]),
        git(&s.lib, &["rev-parse", "origin/main"])
    );
    // `git push` saves it to the group's repository, without a personal remote.
    s.commit(&s.lib, "groups.txt");
    git(&s.lib, &["push", "--quiet"]);
    assert!(s.remote_branch("work/alice-groups").is_some());

    // --here stays; without flags and without interaction, the current branch.
    assert!(run(&["claude", "--here"]).status.success());
    assert_eq!(
        git(&s.lib, &["branch", "--show-current"]),
        "work/alice-groups"
    );

    // From main, without interaction: work/<person>, as before.
    git(&s.lib, &["switch", "--quiet", "main"]);
    assert!(run(&["claude"]).status.success());
    assert_eq!(git(&s.lib, &["branch", "--show-current"]), "work/alice");
    assert!(
        !run(&["claude", "--here", "--branch", "work/alice"])
            .status
            .success()
    );

    // Uncommitted changes: without interaction, it stays, and nothing is lost.
    s.commit(&s.lib, "mine.txt");
    std::fs::write(s.lib.join("mine.txt"), "changed\n").unwrap();
    let out = run(&["claude", "--branch", "work/alice-groups"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(git(&s.lib, &["branch", "--show-current"]), "work/alice");
    assert_eq!(read(s.lib.join("mine.txt")), "changed\n");
    git(&s.lib, &["commit", "--quiet", "-am", "Change mine"]);

    // On a share branch, without interaction, it fails and names the flag.
    git(&s.lib, &["switch", "--quiet", "--create", "share/alice"]);
    let out = run(&["claude"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("pass --branch <name>"),
        "{}",
        stderr(&out)
    );
    let out = run(&["share", "--no-build"]);
    assert!(!out.status.success());
    git(&s.lib, &["switch", "--quiet", "work/alice"]);
    git(&s.lib, &["branch", "--quiet", "-D", "share/alice"]);
}

#[test]
fn only_branches_whose_content_is_in_main_are_deleted() {
    let s = Shared::new("delete-merged");
    let run = |args: &[&str]| {
        let out = s.stemma(&s.lib, args, &[]);
        assert!(out.status.success(), "{}", stderr(&out));
    };
    // work/alice-done is shared and merged; work/alice-live has more work.
    run(&["claude", "--branch", "work/alice-done"]);
    s.commit(&s.lib, "done.txt");
    git(&s.lib, &["push", "--quiet"]);
    assert_eq!(s.share(&[], &[])["ok"], true);
    run(&["claude", "--branch", "work/alice-live"]);
    s.commit(&s.lib, "live.txt");
    git(&s.lib, &["push", "--quiet"]);
    let other = s.clone("merger");
    git(
        &other,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "--no-edit",
            "origin/share/alice-done",
        ],
    );
    git(&other, &["push", "--quiet", "origin", "main"]);
    let forge = read(s.forge.clone()).replace("\"open\"", "\"merged\"");
    std::fs::write(&s.forge, forge).unwrap();
    // The merged branch got one more commit on the remote: it is not all in main.
    git(&other, &["switch", "--quiet", "work/alice-done"]);
    let extra = s.commit(&other, "late.txt");
    git(&other, &["push", "--quiet", "origin", "work/alice-done"]);

    run(&["claude", "--here", "--delete-merged"]);
    assert!(git(&s.lib, &["branch", "--list", "work/alice-done"]).contains("done"));
    assert_eq!(
        s.remote_branch("work/alice-done").as_deref(),
        Some(extra.as_str())
    );

    // Once that is in main too, it goes, here and on the remote, with its share
    // branch; the live branch stays.
    git(&other, &["switch", "--quiet", "main"]);
    git(
        &other,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "--no-edit",
            "work/alice-done",
        ],
    );
    git(&other, &["push", "--quiet", "origin", "main"]);
    run(&["claude", "--here", "--delete-merged"]);
    assert_eq!(git(&s.lib, &["branch", "--list", "work/alice-done"]), "");
    assert_eq!(s.remote_branch("work/alice-done"), None);
    assert_eq!(s.remote_branch("share/alice-done"), None);
    assert!(git(&s.lib, &["branch", "--list", "work/alice-live"]).contains("live"));
    assert!(s.remote_branch("work/alice-live").is_some());
    assert_eq!(
        git(&s.lib, &["branch", "--show-current"]),
        "work/alice-live"
    );
}

#[test]
fn a_personal_remote_keeps_working_branches_off_the_group_repository() {
    let s = Shared::new("personal-remote");
    let personal = s.root.join("personal.git");
    git(&s.root, &["init", "--quiet", "--bare", "personal.git"]);
    let out = s.stemma(
        &s.lib,
        &["remote", personal.to_str().unwrap(), "--json"],
        &[],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["save_remote"], "personal");
    // It is the clone's own configuration, never the library's.
    assert!(!read(s.lib.join("stemma.toml")).contains("personal"));

    assert!(s.stemma(&s.lib, &["claude"], &[]).status.success());
    let head = s.commit(&s.lib, "a.txt");
    git(&s.lib, &["push", "--quiet"]);
    let saved = Command::new("git")
        .args(["rev-parse", "work/alice"])
        .current_dir(&personal)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&saved.stdout).trim(), head);
    assert_eq!(s.remote_branch("work/alice"), None);

    // Sharing goes to the group's repository, as share/alice.
    let v = s.share(&[], &[]);
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(
        s.remote_branch("share/alice").as_deref(),
        Some(head.as_str())
    );
    assert_eq!(s.remote_branch("work/alice"), None);

    let out = s.stemma(&s.lib, &["remote", "--unset", "--json"], &[]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["save_remote"], "origin");
}

#[test]
fn init_and_upgrade_set_the_repository_up_and_say_what_to_set_by_hand() {
    let s = Shared::new("forge-setup");
    let state: serde_json::Value = serde_json::from_str(&read(s.forge.clone())).unwrap();
    assert_eq!(
        state["setup"],
        serde_json::json!(["rules", "merge-commits"])
    );

    // Without permission, upgrading says what to set by hand, and still succeeds.
    std::fs::write(&s.forge, "{\"deny_setup\": true}").unwrap();
    let out = s.stemma(&s.lib, &["upgrade", "--no-update"], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Set it by hand"), "{text}");
    assert!(
        text.contains("disallow squash merging and rebase merging"),
        "{text}"
    );
    assert!(
        text.contains("requires the status check \"Stemma\""),
        "{text}"
    );

    std::fs::write(&s.forge, "{}").unwrap();
    let out = s.stemma(&s.lib, &["upgrade", "--no-update", "--json"], &[]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["forge"][0]["error"], serde_json::Value::Null, "{v}");
    let out = s.stemma(
        &s.lib,
        &["upgrade", "--no-update", "--no-forge", "--json"],
        &[],
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["forge"], serde_json::Value::Null);
}
