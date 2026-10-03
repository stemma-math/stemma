# Code formatter for the repository: `nix fmt` formats every file, and
# `nix flake check` checks that every file is formatted.
{
  pkgs,
  inputs,
  flake,
  ...
}: let
  treefmt = inputs.treefmt-nix.lib.evalModule pkgs {
    projectRootFile = "flake.nix";

    # Nix.
    programs.alejandra.enable = true;

    # Rust, for the edition the workspace uses.
    programs.rustfmt = {
      enable = true;
      edition = "2024";
    };

    # TOML, ordered and validated with the JSON Schemas that tombi ships
    # (Cargo.toml, lakefile.toml, …). Offline, so that it also runs in the
    # build sandbox.
    settings.formatter.tombi = {
      command = pkgs.lib.getExe pkgs.tombi;
      options = ["format" "--offline"];
      includes = ["*.toml"];
    };

    # The Justfile.
    programs.just.enable = true;
  };
in
  treefmt.config.build.wrapper.overrideAttrs (old: {
    passthru =
      old.passthru
      // {
        tests.check = treefmt.config.build.check flake;
      };
  })
