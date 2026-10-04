//! The command line and the Lean library are released together, with one version.

#[test]
fn the_lean_library_has_the_command_line_version() {
    let lakefile = include_str!("../../lean/lakefile.toml");
    let expected = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
    assert!(
        lakefile.lines().any(|line| line.trim() == expected),
        "lean/lakefile.toml must say `{expected}`, as Cargo.toml does"
    );
}
