{
  inputs = {
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    pre-commit-hooks = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [
        ./nix/devshell.nix
        ./nix/packages/realtime-stt-server.nix
        ./nix/packages/whisper-overlay.nix

        # Derive the output overlay automatically from all packages that we define.
        inputs.flake-parts.flakeModules.easyOverlay
      ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      flake = {config, ...}: {
        nixosModules.default = {
          imports = [./nix/nixosModules/realtime-stt-server.nix];
          nixpkgs.overlays = [config.overlays.default];
        };

        homeManagerModules.default = {
          imports = [./nix/homeManagerModules/realtime-stt-server.nix];
          nixpkgs.overlays = [config.overlays.default];
        };
      };

      perSystem = {system, ...}: {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          config.allowUnfree = true;
          #config.cudaSupport = true;
        };
      };
    };
}
