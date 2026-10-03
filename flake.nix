{
  description = "Stemma: a standard and a command line for doing mathematics in Lean with agents";

  inputs = {
    # nixpkgs, the Nix packages collection.
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

    # blueprint, a library for Nix flakes.
    blueprint = {
      url = "github:numtide/blueprint";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # treefmt-nix, treefmt configured with Nix.
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # devshell, a per-project development environment.
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.blueprint {
      inherit inputs;
      prefix = "nix/";

      # nix-systems/default, without x86_64-darwin: nixpkgs dropped it in 26.11.
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];

      # Overlays applied to nixpkgs.
      nixpkgs.overlays = [
        inputs.devshell.overlays.default
      ];
    };
}
