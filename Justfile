# Recipes for working on Stemma. Run `just` to list them.

[private]
default:
    @just --list --unsorted

# Format every file in the repository.
[group('development')]
format:
    nix fmt

alias fmt := format

# Build the command line and the Lean library.
[group('development')]
build:
    cargo build
    cd lean && lake build

# Run the command line with the given arguments.
[group('development')]
run *ARGS:
    cargo run --quiet -- {{ ARGS }}

# Run the Rust tests and the Lean library's tests.
[group('development')]
test:
    cargo test
    cd lean && lake build StemmaTest stemma-extract
    lean/tests/check-failures.sh
    lean/tests/check-extract.sh

# Run every check: formatting, Clippy, the tests and the Lean build.
[group('development')]
check:
    cargo metadata --locked --format-version 1 > /dev/null
    nix flake check
    cargo clippy --all-targets -- --deny warnings
    just test
