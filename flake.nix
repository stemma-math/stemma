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

      # Overlays applied to nixpkgs.
      nixpkgs.overlays = [
        inputs.devshell.overlays.default
      ];
    };
}
