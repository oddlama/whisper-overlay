{inputs, ...}: {
  perSystem = {
    config,
    pkgs,
    ...
  }: let
    libraries = [
      pkgs.libxkbcommon
      pkgs.xdotool
    ];

    includes = [
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
      pkgs.xorg.libX11
    ];
  in {
    overlayAttrs = {
      inherit (config.packages) whisper-overlay;
    };

    packages.default = config.packages.whisper-overlay;
    packages.whisper-overlay = pkgs.rustPlatform.buildRustPackage {
      pname = "whisper-overlay";
      inherit ((builtins.fromTOML (builtins.readFile ../../Cargo.toml)).package) version;

      src = ../../.;
      cargoHash = "sha256-HBbQ14Kxx09qpC5Jwe6mMal0F4NJ+Zb2rl/YvClzVl4=";

      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.wrapGAppsHook4
      ];
      buildInputs =
        includes
        ++ libraries
        ++ [
          pkgs.xorg.xorgproto
        ];
    };

    devshells.default = {
      imports = [
        "${inputs.devshell}/extra/language/c.nix"
        "${inputs.devshell}/extra/language/rust.nix"
      ];

      packages = [
        pkgs.pkg-config
        pkgs.rust-analyzer
      ];

      language.c.libraries = libraries;
      language.c.includes = includes;

      env = [
        {
          # This is what wrapGAppsHook4 would do. Without it, text rendering will be broken for some reason.
          name = "XDG_DATA_DIRS";
          prefix = pkgs.lib.concatStringsSep ":" [
            "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}"
            "${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}"
          ];
        }
        {
          name = "PKG_CONFIG_PATH";
          prefix = pkgs.lib.makeSearchPath "share/pkgconfig" [
            # xorg.xorgproto clashes with some files in xorg.libX11,
            # so we manually add it to the search path as a prefix here
            pkgs.xorg.xorgproto
          ];
        }
      ];
    };
  };
}
