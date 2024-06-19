{
  inputs = {
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = {
    devshell,
    flake-utils,
    nixpkgs,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (localSystem: let
      pkgs = import nixpkgs {
        inherit localSystem;
        overlays = [
          devshell.overlays.default
        ];
      };
    in {
      packages.default = pkgs.rustPlatform.buildRustPackage {
        pname = "whisper-overlay";
        version = "1.0.0";
        src = ./.;
        nativeBuildInputs = [
          pkgs.pkg-config
          pkgs.wrapGAppsHook4
        ];
        buildInputs = [
          pkgs.gtk4
          pkgs.gtk4-layer-shell
          pkgs.pango
          pkgs.glib
          pkgs.cairo
          pkgs.graphene
          pkgs.gdk-pixbuf
          pkgs.harfbuzz
          pkgs.vulkan-loader
        ];
        cargoHash = "sha256-N/Kenj0IdxexjcpL5EmKbbzZFUOxSDFXyylCn141bDU=";
      };

      # `nix develop`
      devShells.default = pkgs.devshell.mkShell {
        name = "whisper-overlay";
        imports = [
          "${devshell}/extra/language/c.nix"
          "${devshell}/extra/language/rust.nix"
        ];

        commands = [
          {
            package = pkgs.alejandra;
            help = "Format nix code";
          }
          {
            package = pkgs.statix;
            help = "Lint nix code";
          }
          {
            package = pkgs.deadnix;
            help = "Find unused expressions in nix code";
          }
        ];

        packages = [
          pkgs.pkg-config
          pkgs.rust-analyzer
        ];

        language.c.libraries = [
          pkgs.libxkbcommon
      ];

        language.c.includes = [
          pkgs.gtk4
          pkgs.gtk4-layer-shell
          pkgs.wayland
          pkgs.pango
          pkgs.glib
          pkgs.cairo
          pkgs.graphene
          pkgs.gdk-pixbuf
          pkgs.harfbuzz
          pkgs.vulkan-loader
          pkgs.alsa-lib
        ];

        env = [
          {
            # This is what wrapGAppsHook4 would do. Without it, text rendering will be broken for some reason.
            name = "XDG_DATA_DIRS";
            prefix = pkgs.lib.concatStringsSep ":" [
              "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}"
              "${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}"
            ];
          }
        ];
      };

      formatter = pkgs.alejandra; # `nix fmt`
    });
}
