# Development shell.
{pkgs, ...}: let
  inherit (pkgs) lib stdenv;
in
  pkgs.devshell.mkShell {
    devshell = {
      name = "stemma";

      packages = [
        pkgs.rustc
        pkgs.clippy
        pkgs.rustfmt
        pkgs.clang
      ];
    };

    env =
      [
        {
          # The standard library sources, for rust-analyzer.
          name = "RUST_SRC_PATH";
          value = "${pkgs.rustPlatform.rustLibSrc}";
        }
      ]
      ++ lib.optionals stdenv.hostPlatform.isDarwin [
        {
          # Link against the nixpkgs SDK, and for the macOS version the
          # nixpkgs Rust standard library is built for.
          name = "SDKROOT";
          value = "${pkgs.apple-sdk.sdkroot}";
        }
        {
          name = "MACOSX_DEPLOYMENT_TARGET";
          value = stdenv.hostPlatform.darwinMinVersion;
        }
      ];

    commands = [
      {
        package = pkgs.git;
        help = "Version control";
      }
      {
        package = pkgs.just;
        help = "Run the repository's recipes; type 'just' to list them";
      }
      {
        package = pkgs.cargo;
        help = "Build, test and run the command line";
      }
      {
        package = pkgs.rust-analyzer;
        help = "Rust language server";
      }
      {
        package = pkgs.elan;
        help = "Lean toolchain manager; the toolchain is pinned in lean/lean-toolchain";
      }
    ];
  }
