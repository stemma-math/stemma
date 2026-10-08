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

/// A git identity, so that commits work wherever the tests run, and no global
/// configuration of git or of `gh` (such as a signing key or a login).
const IDENTITY: [(&str, &str); 7] = [
    ("GIT_AUTHOR_NAME", "Test"),
    ("GIT_AUTHOR_EMAIL", "test@example.com"),
    ("GIT_COMMITTER_NAME", "Test"),
    ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GH_CONFIG_DIR", "/nonexistent"),
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

/// A `lake` that builds nothing and gives, as the library's report, the file
/// `STEMMA_TEST_REPORT` names: the command line's side of the checks, without
/// Lean. Returns the directory to put first in `PATH`.
fn fake_lake(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let lake = bin.join("lake");
    std::fs::write(
        &lake,
        "#!/bin/sh\ncase \"$1\" in\n  build) exit 0 ;;\n  \
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
