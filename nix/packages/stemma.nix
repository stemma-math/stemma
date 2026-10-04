# The `stemma` command line.
{pkgs, ...}: let
  inherit (pkgs) lib;
  root = ../..;
  manifest = lib.importTOML (root + "/Cargo.toml");
in
  pkgs.rustPlatform.buildRustPackage {
    pname = "stemma";
    inherit (manifest.workspace.package) version;

    # Only what Cargo reads, so that changes elsewhere (the Lean library, docs, Nix)
    # do not rebuild the command line.
    src = lib.fileset.toSource {
      inherit root;
      fileset = lib.fileset.unions [
        (root + "/Cargo.toml")
        (root + "/Cargo.lock")
        (root + "/cli")
        # The Lean toolchain the command line works with, and the Lean
        # library's version, which must be the command line's.
        (root + "/lean/lean-toolchain")
        (root + "/lean/lakefile.toml")
      ];
    };

    cargoLock.lockFile = root + "/Cargo.lock";

    # The integration tests make git repositories.
    nativeCheckInputs = [pkgs.git];

    meta = {
      description = "The Stemma command line";
      license = lib.licenses.mit;
      mainProgram = "stemma";
    };
  }
