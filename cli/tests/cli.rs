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
        "references.bib",
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
    let config = read(dir.join("stemma.toml"));
    let config = config[..config.find("[policy]").unwrap()].replace(
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
    std::fs::remove_file(dir.join("references.bib")).unwrap();

    // A dry run changes nothing.
    let out = stemma(&dir, &["upgrade", "--dry-run", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(plan["from"], "0.1.0");
    // From 0.1.0: two migrations of 0.2.0, one of 0.3.0 and one of 0.4.0.
    assert_eq!(plan["migrations"].as_array().unwrap().len(), 4);
    assert!(read(dir.join("stemma.toml")).contains("self_merge"));

    // Without a terminal, it asks nothing, and says what follows: a
    // maintainer's approval with `stemma sign`, which also re-signs what the
    // new versions left stale, then sharing.
    let out = stemma(&dir, &["upgrade", "--no-update"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("`stemma sign`, in a maintainer's terminal, to approve the upgrade"),
        "{text}"
    );
    assert!(
        text.contains("`stemma status` lists them); then `stemma share`"),
        "{text}"
    );
    let config = read(dir.join("stemma.toml"));
    assert!(config.contains(&format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION"))));
    // 0.2.0 renamed the key, and 0.3.0 removed it.
    assert!(!config.contains("self_merge"));
    assert!(!config.contains("merge_without_approval"));
    // 0.4.0 requires signed central environments, explicitly.
    assert!(config.contains("require_signed_central = true"), "{config}");
    assert!(read(dir.join("AGENTS.md")).contains("Working in a Stemma library"));
    assert!(!read(dir.join("lean-toolchain")).contains("v4.20.0"));
    assert!(dir.join(".github/workflows/stemma.yml").is_file());
    // The references, created where missing, and never written over.
    assert_eq!(read(dir.join("references.bib")), "");
    let entry = "@book{K, author = {A}, title = {T}, year = {2000}}\n";
    std::fs::write(dir.join("references.bib"), entry).unwrap();

    // Upgrading again changes nothing.
    let out = stemma(&dir, &["upgrade", "--no-update", "--json"]);
    let again: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(again["changed"].as_array().unwrap().is_empty());
    assert_eq!(read(dir.join("references.bib")), entry);
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
    let next = result["next"].as_str().unwrap();
    assert!(next.starts_with("Next: `stemma sign`"), "{next}");
    assert!(next.ends_with("then `stemma share`."), "{next}");
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
        // Fake agents, so that starting one only chooses the branch. Each
        // records the session variable it got, and its arguments.
        let bin = shared.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for agent in ["claude", "codex", "dsh", "opencode"] {
            let path = bin.join(agent);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho \"STEMMA_SESSION=$STEMMA_SESSION $*\" > \"{}\"\nexit 0\n",
                    shared.root.join(format!("{agent}.started")).display()
                ),
            )
            .unwrap();
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

/// A `lake` that builds nothing and gives, as the library's report, the file
/// `STEMMA_TEST_REPORT` names: the command line's side of the checks, without
/// Lean. Returns the directory to put first in `PATH`.
fn fake_lake(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let lake = bin.join("lake");
    std::fs::write(
        &lake,
        "#!/bin/sh\ncase \"$1\" in\n  \
         build) [ -z \"$STEMMA_TEST_BUILD_FAILS\" ] || { echo \"$STEMMA_TEST_BUILD_FAILS\"; exit 1; } ;;\n  \
         exe) cp \"$STEMMA_TEST_REPORT\" \"$4\" ;;\n  *) exit 1 ;;\nesac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&lake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// A central statement of `Alg.Even`, spanning `lines`, for a report.
struct Claim {
    label: &'static str,
    lines: (u32, u32),
    formal: &'static str,
}

/// Writes the report the fake `lake` gives, with `claims`.
fn write_report(path: &Path, claims: &[Claim]) {
    let environments: Vec<serde_json::Value> = claims
        .iter()
        .map(|c| {
            serde_json::json!({
                "record": {
                    "name": "theorem", "display": "Theorem", "base": "statement",
                    "label": c.label, "central": true, "cited": null, "title": null,
                    "of": null, "decls": [format!("thm_{}", c.label.replace('-', "_"))],
                    "prose": format!("The statement {}.", c.label),
                    "lean": "```lean\ntheorem thm : True := trivial\n```",
                    "module": "Alg.Even", "line": c.lines.0, "endLine": c.lines.1,
                },
                "state": "proved",
                "fingerprints": { "version": 1, "prose": "sha256:p", "formal": c.formal },
                "closure": [],
            })
        })
        .collect();
    let report = serde_json::json!({
        "version": 1, "library": "Alg", "rootDocument": false,
        "modules": [{ "name": "Alg.Even", "kind": "document" }],
        "environments": environments, "diagnostics": [],
    });
    std::fs::write(path, report.to_string()).unwrap();
}

/// The signature file of a claim, as `stemma sign` writes it.
fn signature_of(claim: &Claim) -> String {
    format!(
        "label = \"{}\"\nkind = \"statement\"\ncentral = true\n\nsigner = \"alice\"\n\
         signed = 2026-10-08T00:00:00Z\n\n[fingerprints]\nversion = 1\nprose = \"sha256:p\"\n\
         formal = \"{}\"\n\n[closure]\n",
        claim.label, claim.formal
    )
}

/// `stemma`, with the fake `lake` and its report.
fn stemma_built(dir: &Path, bin: &Path, report: &Path, args: &[&str]) -> std::process::Output {
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(args)
        .envs(IDENTITY)
        .env("PATH", path)
        .env("STEMMA_TEST_REPORT", report)
        .env_remove("STEMMA_SESSION")
        .current_dir(dir)
        .output()
        .unwrap()
}

fn json_of(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A document with `n` lines, so that environments can span some of them.
fn document(n: u32, tag: &str) -> String {
    (1..=n).map(|i| format!("line {i} {tag}\n")).collect()
}

/// A library on `main` with one module, `Alg.Even`, whose members are alice
/// (maintainer and signer, with her key) and bob (signer, with no key yet).
/// Returns the library, alice's private key and her public key.
fn library_of_alice(name: &str) -> (PathBuf, String, String) {
    let dir = scratch(name);
    let keys = scratch(&format!("{name}-keys"));
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
    let (library, rest) = config.split_at(config.find("[members]").unwrap());
    let policy = &rest[rest.find("\n#").unwrap()..];
    std::fs::write(
        dir.join("stemma.toml"),
        format!(
            "{library}[members]\nalice = {{ roles = [\"maintainer\", \"signer\"], keys = [\"{alice_public}\"] }}\n\
             bob = {{ roles = [\"signer\"], keys = [] }}\n{policy}"
        ),
    )
    .unwrap();
    assert!(stemma(&dir, &["new", "Even"]).status.success());
    std::fs::write(dir.join("Alg/Even.lean"), document(10, "main")).unwrap();
    std::fs::write(
        dir.join(".gitignore"),
        "/.lake\n/.stemma\n/bin\n/report.json\n",
    )
    .unwrap();
    git(&dir, &["add", "--all"]);
    git(
        &dir,
        &["commit", "--quiet", "-m", "Alice, and the even numbers"],
    );
    (dir, alice, alice_public)
}

#[test]
fn check_requires_central_environments_to_be_signed() {
    let (dir, _, _) = library_of_alice("required");
    let bin = fake_lake(&dir);
    let report = dir.join("report.json");
    let claim = Claim {
        label: "even-add",
        lines: (3, 8),
        formal: "sha256:f",
    };
    write_report(&report, std::slice::from_ref(&claim));
    let check = |dir: &Path| json_of(&stemma_built(dir, &bin, &report, &["check", "--json"]));

    // Unsigned, it is a problem of `stemma check`, with its label and state.
    let c = check(&dir);
    assert_eq!(c["ok"], false, "{c}");
    // The build passed, and says so: it is never `null`.
    assert_eq!(c["build"]["ok"], true, "{c}");
    assert!(c["build"]["seconds"].is_number(), "{c}");
    let problems = c["problems"].to_string();
    assert!(
        problems.contains("'even-add' is central and unsigned"),
        "{problems}"
    );
    // `stemma status` lists it, and warns about bob, a signer without a key.
    let s = json_of(&stemma_built(&dir, &bin, &report, &["status", "--json"]));
    assert_eq!(s["awaiting_signature"][0]["label"], "even-add", "{s}");
    assert_eq!(s["awaiting_signature"][0]["signature"], "unsigned", "{s}");
    assert!(s["warnings"].to_string().contains("bob"), "{s}");

    // Signed, it is not.
    std::fs::create_dir_all(dir.join("signatures")).unwrap();
    std::fs::write(dir.join("signatures/even-add.toml"), signature_of(&claim)).unwrap();
    assert_eq!(check(&dir)["ok"], true);

    // Changed since it was signed, it is stale.
    let changed = Claim {
        formal: "sha256:g",
        ..claim
    };
    write_report(&report, std::slice::from_ref(&changed));
    let problems = check(&dir)["problems"].to_string();
    assert!(
        problems.contains("'even-add' is central and its signature is stale"),
        "{problems}"
    );

    // The policy is read from the base: a change cannot turn it off for itself.
    std::fs::remove_file(dir.join("signatures/even-add.toml")).unwrap();
    git(&dir, &["update-ref", "refs/remotes/origin/main", "main"]);
    git(&dir, &["switch", "--quiet", "--create", "work"]);
    let config = read(dir.join("stemma.toml")).replace(
        "require_signed_central = true",
        "require_signed_central = false",
    );
    std::fs::write(dir.join("stemma.toml"), &config).unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Turn it off"]);
    assert_eq!(check(&dir)["ok"], false);
    // Once the base turns it off, nothing is required.
    git(&dir, &["update-ref", "refs/remotes/origin/main", "work"]);
    assert_eq!(check(&dir)["ok"], true);
}

#[test]
fn check_reports_a_failed_build_with_its_log() {
    let (dir, _, _) = library_of_alice("build-fails");
    let bin = fake_lake(&dir);
    let report = dir.join("report.json");
    write_report(&report, &[]);
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(["check", "--json"])
        .envs(IDENTITY)
        .env("PATH", path)
        .env("STEMMA_TEST_REPORT", &report)
        .env(
            "STEMMA_TEST_BUILD_FAILS",
            "error: Alg/Even.lean:3:0: unknown identifier 'foo'",
        )
        .env_remove("STEMMA_SESSION")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let c = json_of(&out);
    assert_eq!(c["ok"], false, "{c}");
    assert_eq!(c["build"]["ok"], false, "{c}");
    assert!(
        c["build"]["log"]
            .as_str()
            .unwrap()
            .contains("unknown identifier 'foo'"),
        "{c}"
    );
    assert!(c["build"].get("seconds").is_none(), "{c}");
}

#[test]
fn share_counts_what_the_branch_adds() {
    let (dir, _, _) = library_of_alice("scope");
    let remote = scratch("scope-remote");
    git(
        &remote,
        &["init", "--quiet", "--bare", "--initial-branch=main"],
    );
    git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(&dir, &["push", "--quiet", "origin", "main"]);
    // On the branch, a new environment below the one already on main, and a
    // change inside it.
    git(&dir, &["switch", "--quiet", "--create", "work"]);
    let mut text = document(10, "main");
    text.push_str(&document(8, "work"));
    std::fs::write(dir.join("Alg/Even.lean"), text).unwrap();
    git(
        &dir,
        &[
            "commit",
            "--quiet",
            "-am",
            "Odd plus odd",
            "-m",
            "Agent: test-agent",
        ],
    );
    let bin = fake_lake(&dir);
    let report = dir.join("report.json");
    write_report(
        &report,
        &[
            Claim {
                label: "old",
                lines: (2, 6),
                formal: "sha256:o",
            },
            Claim {
                label: "new",
                lines: (12, 16),
                formal: "sha256:n",
            },
        ],
    );
    let out = stemma_built(&dir, &bin, &report, &["share", "--json"]);
    let s = json_of(&out);
    assert_eq!(s["pushed"], true, "{s}");
    assert_eq!(s["new_environments"]["labels"], serde_json::json!(["new"]));
    assert_eq!(s["new_environments"]["central"], 1);
    // Both lack a signature, which the person (with no key) cannot give.
    let others = s["needs_others"].to_string();
    assert!(
        others.contains("'old' is central and unsigned")
            && others.contains("'new' is central and unsigned"),
        "{others}"
    );
}

#[test]
fn a_new_key_needs_another_maintainers_approval() {
    let (dir, alice, _) = library_of_alice("key-add");
    let keys = scratch("key-add-bob");
    let (bob, bob_public) = ssh_key(&keys, "bob");
    // bob signs, by his role, but has no key: a warning, not a failure.
    let v = verify(&dir);
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["warnings"].to_string().contains("bob"), "{v}");

    // He registers his key; on main, the change goes to a branch.
    let main = git(&dir, &["rev-parse", "main"]);
    let bob_pub_path = format!("{bob}.pub");
    let out = stemma(
        &dir,
        &[
            "key",
            "add",
            "--key",
            &bob_pub_path,
            "--member",
            "bob",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["branch"], "keys/bob");
    assert_eq!(
        git(&dir, &["rev-parse", "main"]),
        main,
        "main must not move"
    );
    let key = bob_public
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(read(dir.join("stemma.toml")).contains(&key));
    assert!(
        !stemma(
            &dir,
            &["key", "add", "--key", &bob_pub_path, "--member", "bob"]
        )
        .status
        .success(),
        "the key is already his"
    );

    // It is a change of the policy, and bob cannot approve his own first key.
    let v = verify(&dir);
    assert!(
        v["missing"].to_string().contains("\"change\":\"policy\""),
        "{v}"
    );
    approve(&dir, &bob, "policy");
    let v = verify(&dir);
    assert_eq!(v["ok"], false);
    assert!(v["approvals"].get("policy").is_none(), "{v}");
    assert!(
        v["missing"].to_string().contains("\"change\":\"policy\""),
        "{v}"
    );

    // Alice, a maintainer whose key is on main, can.
    git(&dir, &["reset", "--quiet", "--hard", "HEAD~1"]);
    approve(&dir, &alice, "policy");
    let v = verify(&dir);
    assert_eq!(v["ok"], true, "{v}");
}

/// A process in a pseudo-terminal: `stemma sign` runs only in one.
#[cfg(unix)]
mod pty {
    use std::ffi::CStr;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub struct Pty {
        master: File,
        output: Arc<Mutex<Vec<u8>>>,
        pub child: Child,
    }

    impl Pty {
        /// Runs `command` with a terminal of 100 columns as its standard
        /// streams, or gives `None` where no terminal can be made.
        pub fn spawn(mut command: Command) -> Option<Self> {
            // SAFETY: plain calls to the C library on descriptors this
            // function owns.
            let (master, slave) = unsafe {
                let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
                if master < 0 || libc::grantpt(master) != 0 || libc::unlockpt(master) != 0 {
                    return None;
                }
                let name = CStr::from_ptr(libc::ptsname(master)).to_owned();
                let slave = libc::open(name.as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
                if slave < 0 {
                    return None;
                }
                let size = libc::winsize {
                    ws_row: 60,
                    ws_col: 100,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                libc::ioctl(master, libc::TIOCSWINSZ, &size);
                (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave))
            };
            command
                .stdin(Stdio::from(slave.try_clone().ok()?))
                .stdout(Stdio::from(slave.try_clone().ok()?))
                .stderr(Stdio::from(slave));
            let child = command.spawn().ok()?;
            let output = Arc::new(Mutex::new(Vec::new()));
            let mut reader = master.try_clone().ok()?;
            let sink = output.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    sink.lock().unwrap().extend_from_slice(&buf[..n]);
                }
            });
            Some(Self {
                master,
                output,
                child,
            })
        }

        /// Everything the process has written so far.
        pub fn output(&self) -> String {
            String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
        }

        /// Waits until the process has written `text`.
        pub fn expect(&self, text: &str) {
            let start = Instant::now();
            while !self.output().contains(text) {
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "waited for {text:?}; got:\n{}",
                    self.output()
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            // Let the prompt start reading keys.
            std::thread::sleep(Duration::from_millis(300));
        }

        /// Types keys.
        pub fn send(&mut self, keys: &str) {
            for c in keys.chars() {
                write!(self.master, "{c}").unwrap();
                self.master.flush().unwrap();
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        /// Waits for the process to end, and gives its output.
        pub fn finish(mut self) -> (bool, String) {
            let start = Instant::now();
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    std::thread::sleep(Duration::from_millis(200));
                    return (status.success(), self.output());
                }
                if start.elapsed() > Duration::from_secs(30) {
                    let _ = self.child.kill();
                    panic!("the process did not end:\n{}", self.output());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn sign_separates_this_branch_from_others_and_signs_only_what_is_chosen() {
    let (dir, alice, _) = library_of_alice("sign");
    // On main, bob's agent left an environment unsigned.
    std::fs::write(dir.join("Alg/Even.lean"), document(10, "bob")).unwrap();
    git(
        &dir,
        &[
            "commit",
            "--quiet",
            "-am",
            "Even numbers",
            "-m",
            "Agent: bobs-agent",
            "--author",
            "Bob <bob@example.com>",
        ],
    );
    // On alice's branch, two new environments.
    git(&dir, &["switch", "--quiet", "--create", "work/alice"]);
    let mut text = document(10, "bob");
    text.push_str(&document(12, "alice"));
    std::fs::write(dir.join("Alg/Even.lean"), text).unwrap();
    git(&dir, &["commit", "--quiet", "-am", "Sums and products"]);
    git(&dir, &["config", "gpg.format", "ssh"]);
    git(&dir, &["config", "user.signingkey", &alice]);
    let before = git(&dir, &["rev-parse", "HEAD"]);

    let bin = fake_lake(&dir);
    let report = dir.join("report.json");
    write_report(
        &report,
        &[
            Claim {
                label: "even",
                lines: (2, 8),
                formal: "sha256:e",
            },
            Claim {
                label: "even-add",
                lines: (12, 16),
                formal: "sha256:a",
            },
            Claim {
                label: "even-mul",
                lines: (17, 22),
                formal: "sha256:m",
            },
        ],
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_stemma"));
    command
        .args(["sign", "--base", "main"])
        .envs(IDENTITY)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("STEMMA_TEST_REPORT", &report)
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("STEMMA_SESSION")
        .current_dir(&dir);
    let Some(mut sign) = pty::Pty::spawn(command) else {
        eprintln!("no pseudo-terminal here: skipping");
        return;
    };
    // This branch's are chosen at first: alice leaves out the first.
    sign.expect("Sign which?");
    sign.send(" \r");
    // Others' are not: she chooses none.
    sign.expect("Sign any of these too?");
    sign.send("\r");
    let (ok, output) = sign.finish();
    assert!(ok, "{output}");
    let this_branch = output.find("This branch: 2").expect(&output);
    let others = output.find("Left pending by others: 1").expect(&output);
    assert!(this_branch < others, "{output}");
    assert!(
        output.contains("last changed by Bob (bobs-agent)"),
        "{output}"
    );

    // One commit, signed, with only the chosen signature.
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]), before);
    assert_eq!(
        git(&dir, &["show", "--name-only", "--format=", "HEAD"]),
        "signatures/even-mul.toml"
    );
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "Sign even-mul");
    assert!(!dir.join("signatures/even-add.toml").exists());
    let v = verify(&dir);
    assert!(
        !v["failures"]
            .to_string()
            .contains("not signed with the key"),
        "{v}"
    );
}

#[test]
fn upgrade_in_a_terminal_lists_what_it_will_do_and_asks() {
    let dir = scratch("upgrade-asks");
    let out = stemma(
        &dir,
        &["init", ".", "--name", "Alg", "--no-git", "--no-mathlib"],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let config = read(dir.join("stemma.toml")).replace(
        &format!("stemma = \"{}\"", env!("CARGO_PKG_VERSION")),
        "stemma = \"0.1.0\"",
    );
    std::fs::write(dir.join("stemma.toml"), &config).unwrap();
    let run = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_stemma"));
        command
            .args(["upgrade", "--no-update", "--no-forge"])
            .envs(IDENTITY)
            .env_remove("STEMMA_SESSION")
            .current_dir(&dir);
        pty::Pty::spawn(command)
    };
    // Declining changes nothing.
    let Some(mut upgrade) = run() else {
        eprintln!("no pseudo-terminal here: skipped");
        return;
    };
    upgrade.expect("Upgrade the library?");
    assert!(upgrade.output().contains("From stemma 0.1.0"));
    assert!(upgrade.output().contains("Write stemma.toml."));
    upgrade.send("n");
    let (ok, output) = upgrade.finish();
    assert!(!ok, "{output}");
    assert!(output.contains("Nothing was changed."), "{output}");
    assert_eq!(read(dir.join("stemma.toml")), config);
    // Accepting upgrades, and says what follows.
    let mut upgrade = run().unwrap();
    upgrade.expect("Upgrade the library?");
    upgrade.send("y");
    let (ok, output) = upgrade.finish();
    assert!(ok, "{output}");
    assert!(output.contains("`stemma sign`"), "{output}");
    assert!(read(dir.join("stemma.toml")).contains(env!("CARGO_PKG_VERSION")));
}

/// `stemma agent <harness> --dry-run` in a new library: what it would run.
fn dry_run(dir: &Path, harness: &str) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(["agent", harness, "--dry-run"])
        .envs(IDENTITY)
        .env_remove("STEMMA_SESSION")
        .current_dir(dir)
        .output()
        .unwrap();
    json_of(&out)
}

#[test]
fn every_harness_gets_the_equipment_and_the_session_variable() {
    let dir = scratch("harnesses");
    let out = stemma(&dir, &["init", ".", "--name", "Alg", "--no-mathlib"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let agent = dir.join(".stemma/agent");
    let skills = [
        "stemma-mathematics",
        "stemma-documents",
        "stemma-sharing",
        "stemma-signatures",
    ];

    // The neutral equipment, for any harness, and to equip one by hand.
    let claude = dry_run(&dir, "claude");
    assert!(
        read(agent.join("equipment/instructions.md")).starts_with("# Working in a Stemma library")
    );
    for skill in skills {
        assert!(
            agent
                .join(format!("equipment/skills/{skill}/SKILL.md"))
                .is_file()
        );
    }
    for harness in ["claude", "codex", "deepseek", "opencode"] {
        let v = dry_run(&dir, harness);
        assert_eq!(v["harness"], harness);
        assert_eq!(v["env"]["STEMMA_SESSION"], "1", "{harness}: {v}");
    }

    // Claude Code: settings, a plugin with the skills, the instructions.
    assert_eq!(claude["program"], "claude");
    let args: Vec<&str> = claude["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert!(args.contains(&"--plugin-dir") && args.contains(&"--append-system-prompt-file"));
    let settings: serde_json::Value =
        serde_json::from_str(&read(agent.join("claude/settings.json"))).unwrap();
    assert!(
        settings["permissions"]["deny"]
            .to_string()
            .contains("Bash(stemma sign:*)")
    );
    assert_eq!(settings["env"]["STEMMA_SESSION"], "1");
    for skill in skills {
        assert!(
            agent
                .join(format!("claude/plugin/skills/{skill}/SKILL.md"))
                .is_file()
        );
    }

    // Codex: the instructions and the skills, inline.
    let codex = dry_run(&dir, "codex");
    assert_eq!(codex["program"], "codex");
    let config = codex["args"][3].as_str().unwrap();
    assert!(config.starts_with("developer_instructions="));
    assert!(
        config.contains("Working in a Stemma library") && config.contains("Saving and sharing")
    );

    // DeepSeek Harness: a patch layer with the instructions, skills and hooks.
    let dsh = dry_run(&dir, "deepseek");
    assert_eq!(dsh["program"], "dsh");
    assert_eq!(dsh["args"][0], "web");
    assert_eq!(dsh["args"][1], "--patch");
    let patch_path = PathBuf::from(dsh["args"][2].as_str().unwrap());
    let patch = read(patch_path.clone());
    let patch: serde_json::Value =
        serde_json::from_str(&patch[patch.find('[').unwrap()..]).unwrap();
    let home = PathBuf::from(patch[0]["config"]["dshHome"].as_str().unwrap());
    assert!(read(home.join("AGENTS.md")).starts_with("# Working in a Stemma library"));
    let skills_dir = PathBuf::from(patch[1]["config"]["customSkillDirs"][0].as_str().unwrap());
    for skill in skills {
        assert!(skills_dir.join(skill).join("SKILL.md").is_file());
    }
    let hooks: serde_json::Value = serde_json::from_str(&read(PathBuf::from(
        patch[2]["insert"][0]["config"]["configPath"]
            .as_str()
            .unwrap(),
    )))
    .unwrap();
    assert!(
        hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("hook pre-tool-use")
    );
    let out = Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(["agent", "deepseek", "--dry-run", "--", "--profile", "tui"])
        .envs(IDENTITY)
        .current_dir(&dir)
        .output()
        .unwrap();
    let v = json_of(&out);
    assert_eq!(
        v["args"],
        serde_json::json!(["--patch", patch_path, "--profile", "tui"])
    );

    // OpenCode: its configuration, inline, with the instructions and the
    // session's summary, the skills, and permissions.
    let opencode = dry_run(&dir, "opencode");
    assert_eq!(opencode["program"], "opencode");
    let config: serde_json::Value =
        serde_json::from_str(opencode["env"]["OPENCODE_CONFIG_CONTENT"].as_str().unwrap()).unwrap();
    let instructions = read(PathBuf::from(config["instructions"][0].as_str().unwrap()));
    assert!(instructions.starts_with("# Working in a Stemma library"));
    assert!(instructions.contains("This session is equipped by stemma"));
    let skills_dir = PathBuf::from(config["skills"]["paths"][0].as_str().unwrap());
    for skill in skills {
        assert!(skills_dir.join(skill).join("SKILL.md").is_file());
    }
    assert_eq!(config["permission"]["edit"]["signatures/*"], "deny");
    assert_eq!(config["permission"]["bash"]["stemma sign *"], "deny");
}

#[test]
fn harnesses_start_in_the_session_with_their_arguments() {
    let s = Shared::new("harness-start");
    git(&s.lib, &["switch", "--quiet", "--create", "work/alice"]);
    for (harness, program) in [
        ("deepseek", "dsh"),
        ("opencode", "opencode"),
        ("codex", "codex"),
    ] {
        let out = s.stemma(&s.lib, &["agent", harness, "--here", "--", "--extra"], &[]);
        assert!(out.status.success(), "{}", stderr(&out));
        let started = read(s.root.join(format!("{program}.started")));
        assert!(started.starts_with("STEMMA_SESSION=1 "), "{started}");
        assert!(started.trim_end().ends_with("--extra"), "{started}");
    }
    // The shortcuts start the same harnesses.
    assert!(
        s.stemma(&s.lib, &["claude", "--here"], &[])
            .status
            .success()
    );
    assert!(read(s.root.join("claude.started")).contains("--plugin-dir"));
}

/// `stemma hook <event>` in `dir`, with `input` on stdin.
fn hook(dir: &Path, args: &[&str], input: &str) -> String {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_stemma"))
        .args(args)
        .envs(IDENTITY)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn hooks_give_the_summary_and_deny_protected_files_and_signing() {
    let dir = scratch("hooks");
    let out = stemma(&dir, &["init", ".", "--name", "Alg", "--no-mathlib"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let cwd = dir.display().to_string();
    let call = |tool: &str, input: serde_json::Value| {
        let call = serde_json::json!({ "tool_name": tool, "tool_input": input, "cwd": cwd });
        hook(&dir, &["hook", "pre-tool-use"], &call.to_string())
    };
    let denied = |out: String| {
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny", "{v}");
    };
    denied(call(
        "write",
        serde_json::json!({ "file_path": "stemma.toml", "content": "" }),
    ));
    denied(call(
        "Edit",
        serde_json::json!({ "file_path": format!("{cwd}/signatures/even.toml") }),
    ));
    denied(call(
        "str_replace_editor",
        serde_json::json!({ "command": "create", "path": ".github/x.yml" }),
    ));
    denied(call(
        "bash",
        serde_json::json!({ "command": "cd . && stemma sign even" }),
    ));
    denied(call(
        "Bash",
        serde_json::json!({ "command": "git push --force-with-lease" }),
    ));
    assert_eq!(
        call("write", serde_json::json!({ "file_path": "Alg/Even.lean" })),
        ""
    );
    assert_eq!(
        call(
            "str_replace_editor",
            serde_json::json!({ "command": "view", "path": "stemma.toml" })
        ),
        ""
    );
    assert_eq!(
        call(
            "bash",
            serde_json::json!({ "command": "stemma check --json" })
        ),
        ""
    );
    assert_eq!(
        call("read", serde_json::json!({ "file_path": "stemma.toml" })),
        ""
    );

    let plain = hook(&dir, &["hook", "session-start"], "");
    assert!(plain.starts_with("This session is equipped by stemma"));
    let v: serde_json::Value =
        serde_json::from_str(&hook(&dir, &["--json", "hook", "session-start"], "")).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(
        v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("equipped by stemma")
    );
}
